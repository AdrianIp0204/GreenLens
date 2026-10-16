// Original GreenLens v0.8 complete-register issue presentation. Apache-2.0.
// Fixed owned template; retained text is never evaluated as executable markup.
#let d = json("document.json")
#assert(d.schema_version == "greenlens.energy-document.v0.8")
#let executive = d.kind == "Executive"
#let appendix = d.kind == "Input appendix"
#let ink = rgb("213834")
#let green = rgb("246954")
#let pale = rgb("eef4f0")
#let literal(value) = text(value.replace(regex("\\S{28,}"), m => m.text.clusters().join("\u{200b}")))
#set document(title: d.title, date: none)
#set text(font: "Noto Sans CJK TC", size: if executive { 8.5pt } else { 9pt }, fill: ink, lang: "en")
#set page(paper: "a4", margin: (top: 17mm, bottom: 18mm, x: 16mm),
  header: text(size: 7pt, fill: green)[GREENLENS / ENERGY STUDY ISSUE / #text(d.kind)],
  footer: context grid(columns: (1fr, auto), align: (left, right),
    text(size: 7pt, d.report_id + " · " + d.revision), counter(page).display("1 / 1", both: true)))
#set par(leading: 0.5em)
#show heading.where(level: 1): it => block(above: 10pt, below: 5pt, text(size: if executive { 13pt } else { 15pt }, fill: green, weight: "bold", it))
#show figure.caption: set text(size: 8pt)
#text(size: 22pt, weight: "bold", d.title)
#v(2mm)
#text(size: 11pt, fill: green)[#text(d.kind) · #text(d.revision)]
#v(2mm)
#block(fill: pale, inset: 7pt, width: 100%)[#text(d.scope)]
#v(2mm)
#text(size: 7pt)[Immutable issue content SHA-256: #literal(d.dataset_sha256)]
#parbreak()
#text(size: 7pt)[Execution, integrity, input relationship, documentation and scientific assessment are distinct. Issuance does not establish approval or certification.]
#if not executive {
  v(3mm)
  // Contents flows with the document; no forced mostly empty title/contents page.
  outline(title: [Contents], depth: 1, indent: 8pt)
  v(4mm)
}
#for s in d.sections [
  #heading(level: 1, text(s.title))
  #for p in s.paragraphs [#text(p) #parbreak()]
  #if s.image != none {
    assert(s.image in ("baseline-model.svg", "proposed-model.svg", "monthly.svg", "annual.svg") or s.image.match(regex("^schedule-(baseline|proposed)-[0-9]{3}\\.svg$")) != none or s.image.match(regex("^aligned-view-00[1-8]\\.svg$")) != none)
    figure(image(s.image, width: 100%), caption: text(s.caption), placement: none)
  }
  #if s.headers.len() > 0 and s.rows.len() == 0 { text("No records supplied. Absence is not evidence of completeness or a zero result.") }
  #if s.headers.len() > 0 and s.rows.len() > 0 {
    let widths = if s.title == "Source-to-analysis mapping snapshot" { (29%, 13%, 25%, 33%) }
      else if s.title == "Supported input provenance: source and applied values" { (34%, 27%, 39%) }
      else if s.title == "Assumptions and declared source values" { (32%, 27%, 41%) }
      else if s.headers.len() == 2 { (34%, 66%) }
      else { s.headers.len() }
    set text(size: if appendix { 8pt } else if executive { 8pt } else { 8.5pt })
    table(columns: widths, inset: if executive { 3pt } else { 4pt },
      stroke: 0.35pt + rgb("cbd9d1"),
      fill: (x, y) => if y == 0 { pale } else if calc.odd(y) { white } else { rgb("f8faf9") },
      table.header(repeat: true, ..s.headers.map(h => text(weight: "bold", h))),
      ..s.rows.map(row => row.map(value => literal(value))).flatten())
  }
]
