import {useEffect, useRef, useState} from 'react';
import type {PDFDocumentLoadingTask, PDFDocumentProxy, PDFPageProxy, PDFWorker, RenderTask} from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';

export const ISSUED_PDF_LIMITS = Object.freeze({bytes: 8 * 1024 * 1024, pages: 160, pixels: 8_000_000, loadMs: 30_000, pageMs: 15_000});
type Props = {bytes: Uint8Array; identity: string; label: string; onClose: () => void};
type Loaded = {identity: string; document: PDFDocumentProxy};
// Only retained, backend-verified owned report bytes enter this component.
// No URL, attachments, annotation layer, form layer or PDF scripting sandbox.
class NoAuxiliaryFetch {
  async fetch(): Promise<never> { throw new Error('This offline issued-PDF preview does not fetch external PDF resources.'); }
}
export function IssuedPdfPreview({bytes, identity, label, onClose}: Props) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const abortPreview = useRef<{identity: string; fail: (message: string) => void} | undefined>(undefined);
  const pendingRender = useRef<{identity: string; promise: Promise<void>} | undefined>(undefined);
  useEffect(() => {
    const previous = document.activeElement;
    return () => {if (previous instanceof HTMLElement && previous.isConnected) previous.focus();};
  }, []);
  const [loaded, setLoaded] = useState<Loaded>();
  const [page, setPage] = useState(1);
  const [zoom, setZoom] = useState(1);
  const [state, setState] = useState('Loading retained PDF…');
  const [error, setError] = useState('');
  const [rendered, setRendered] = useState(false);
  useEffect(() => {
    let active = true, task: PDFDocumentLoadingTask | undefined, worker: PDFWorker | undefined, port: Worker | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setLoaded(undefined); setPage(1); setZoom(1); setError(''); setRendered(false); setState('Loading retained PDF…');
    closeButton.current?.focus();
    const dispose = () => {
      if (timer) clearTimeout(timer);
      void task?.destroy().catch(() => {});
      worker?.destroy(); port?.terminate();
    };
    const fail = (message: string) => { if (active) {active = false; setError(message); setState('Preview unavailable'); setLoaded(undefined); setRendered(false); dispose();} };
    abortPreview.current = {identity, fail};
    timer = setTimeout(() => fail('PDF preview exceeded its 30-second loading budget. Save the verified copy for a local viewer.'), ISSUED_PDF_LIMITS.loadMs);
    void (async () => {
      if (!(bytes instanceof Uint8Array) || bytes.byteLength === 0 || bytes.byteLength > ISSUED_PDF_LIMITS.bytes) {
        throw new Error('Inline preview supports retained PDFs up to 8 MiB. Use verified Save Copy or Open Folder for this product.');
      }
      if (!identity || identity.length > 1024 || new TextDecoder('ascii').decode(bytes.subarray(0, 5)) !== '%PDF-') {
        throw new Error('The verified product identity or PDF signature is missing. Reinspect the issued product.');
      }
      const pdf = await import('pdfjs-dist');
      if (!active) return;
      // A dedicated actual worker is required; do not silently parse on the UI thread.
      port = new Worker(workerUrl, {type: 'module', name: 'GreenLens issued PDF'});
      worker = pdf.PDFWorker.create({port});
      const options = {
        data: bytes.slice(), worker,
        // v6.4.299 has no eval/new Function path and removed this former option.
        // Keep the policy explicit; safety does not rely on an ignored option.
        isEvalSupported: false, useWasm: false, useSystemFonts: false,
        disableFontFace: true, useWorkerFetch: false, BinaryDataFactory: NoAuxiliaryFetch,
        enableXfa: false, stopAtErrors: true, disableRange: true, disableStream: true,
        disableAutoFetch: true, isOffscreenCanvasSupported: false, isImageDecoderSupported: false,
        maxImageSize: ISSUED_PDF_LIMITS.pixels, canvasMaxAreaInBytes: ISSUED_PDF_LIMITS.pixels * 4,
      };
      task = pdf.getDocument(options);
      task.onPassword = () => fail('Encrypted PDFs cannot be previewed here. The owned report products should be unencrypted.');
      const document = await task.promise;
      if (!active) { await task.destroy(); return; }
      if (!Number.isInteger(document.numPages) || document.numPages < 1 || document.numPages > ISSUED_PDF_LIMITS.pages) {
        throw new Error('Inline preview supports 1–160 pages. Save the complete verified PDF for a local viewer.');
      }
      if (timer) clearTimeout(timer);
      setLoaded({identity, document}); setState(`Loaded ${document.numPages} pages; rendering page 1…`);
    })().catch(e => fail(e instanceof Error ? e.message : 'PDF preview failed. Save the verified copy for a local viewer.'));
    return () => { active = false; dispose(); if (abortPreview.current?.identity === identity) abortPreview.current = undefined; if (canvas.current) {canvas.current.width = 1; canvas.current.height = 1;} };
  }, [bytes, identity]);
  useEffect(() => {
    if (!loaded || loaded.identity !== identity || !canvas.current) return;
    let active = true, task: RenderTask | undefined, currentPage: PDFPageProxy | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const target = canvas.current;
    setRendered(false); setError(''); setState(`Rendering page ${page} of ${loaded.document.numPages}…`);
    const fail = (message: string) => {if (active) {active = false; task?.cancel(); target.width = 1; target.height = 1; if (abortPreview.current?.identity === identity) abortPreview.current.fail(message);}};
    timer = setTimeout(() => fail('Page loading or rendering exceeded its 15-second budget. The complete verified PDF remains available to save.'), ISSUED_PDF_LIMITS.pageMs);
    const previous = pendingRender.current?.identity === identity ? pendingRender.current.promise : undefined;
    const rendering = (async () => {
      await previous;
      if (!active) return;
      currentPage = await loaded.document.getPage(page);
      if (!active) {currentPage.cleanup(); return;}
      const original = currentPage.getViewport({scale: 1});
      if (![original.width, original.height].every(v => Number.isFinite(v) && v > 0)) throw new Error('PDF page has invalid dimensions.');
      const dpr = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
      const scale = Math.min(zoom * dpr, Math.sqrt(ISSUED_PDF_LIMITS.pixels / (original.width * original.height)) * 0.999);
      const viewport = currentPage.getViewport({scale});
      const width = Math.ceil(viewport.width), height = Math.ceil(viewport.height);
      if (width * height > ISSUED_PDF_LIMITS.pixels) throw new Error('PDF page exceeds the 8-million-pixel preview budget.');
      target.width = width; target.height = height;
      target.style.width = `${width / dpr}px`; target.style.height = `${height / dpr}px`;
      task = currentPage.render({canvas: target, viewport, annotationMode: 0, background: '#ffffff'});
      await task.promise;
      if (timer) clearTimeout(timer);
      if (active) {setRendered(true); setState(`Page ${page} of ${loaded.document.numPages} rendered${scale < zoom * dpr ? ' at the pixel budget limit' : ''}.`);}
    })().catch(e => {if (active) fail(e instanceof Error ? e.message : 'Page rendering failed.');}).finally(() => {if (timer) clearTimeout(timer); currentPage?.cleanup();});
    pendingRender.current = {identity, promise: rendering};
    return () => {active = false; if (timer) clearTimeout(timer); task?.cancel();};
  }, [loaded, identity, page, zoom]);
  const pdfDocument = loaded?.identity === identity ? loaded.document : undefined;
  return <section role="dialog" aria-modal="true" aria-label={`Issued PDF preview: ${label}`} onKeyDown={e => {
    if (e.key === 'Escape') {e.preventDefault(); e.stopPropagation(); onClose();}
    if (e.key === 'Tab') {
      const controls = [...e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex="0"]')];
      const first = controls[0], last = controls.at(-1);
      if (e.shiftKey && document.activeElement === first) {e.preventDefault(); last?.focus();}
      if (!e.shiftKey && document.activeElement === last) {e.preventDefault(); first?.focus();}
    }
  }} style={{position: 'fixed', inset: '4vh 4vw', zIndex: 10000, background: 'var(--surface, #fff)', color: 'var(--text, #213834)', border: '1px solid #7a9588', boxShadow: '0 8px 40px #0006', padding: 16, display: 'flex', flexDirection: 'column', gap: 8}}>
    <header style={{display: 'flex', alignItems: 'center', gap: 12}}><strong style={{flex: 1, overflowWrap: 'anywhere'}}>{label}</strong><button ref={closeButton} onClick={onClose} aria-label="Close issued PDF preview">Close</button></header>
    <p style={{margin: 0}}>Offline preview of the selected retained PDF. Pixel rendering is not a report-integrity or scientific-qualification check. Bounds: 8 MiB, 160 pages, 8 million pixels per page or raster image; oversized raster images may be omitted. Save Copy retains the complete original PDF bytes.</p>
    <div role="toolbar" aria-label="PDF page navigation" style={{display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap'}}>
      <button disabled={!pdfDocument || page <= 1} onClick={() => setPage(p => p - 1)}>Previous page</button>
      <label>Page <input aria-label="PDF page number" type="number" min={1} max={pdfDocument?.numPages ?? 1} value={page} disabled={!pdfDocument} onChange={e => {const n = Number(e.target.value); if (pdfDocument && Number.isInteger(n) && n >= 1 && n <= pdfDocument.numPages) setPage(n);}} style={{width: 70}}/></label>
      <span>of {pdfDocument?.numPages ?? '—'}</span><button disabled={!pdfDocument || page >= pdfDocument.numPages} onClick={() => setPage(p => p + 1)}>Next page</button>
      <label>Zoom <select aria-label="PDF zoom" value={zoom} disabled={!pdfDocument} onChange={e => setZoom(Number(e.target.value))}>{[0.5, 0.75, 1, 1.5, 2].map(z => <option key={z} value={z}>{z * 100}%</option>)}</select></label>
    </div>
    <p role="status" aria-live="polite" style={{margin: 0}}>{state}</p>{error && <p role="alert" style={{margin: 0}}>{error}</p>}
    <div style={{overflow: 'auto', flex: 1, minHeight: 80, background: '#c8d1cc', padding: 12}}><canvas ref={canvas} aria-label={`Rendered PDF page ${page}`} style={{display: rendered ? 'block' : 'none', margin: '0 auto', background: '#fff'}}/></div>
  </section>;
}
