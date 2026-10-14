// Original GreenLens v0.5 report presentation. Apache-2.0.
// Rust owns values, comparisons and disclosures. Text is never evaluated.
#let d = json("document.json")
#assert(d.schema_version == "greenlens.energy-document.v0.5")
#let executive = d.kind == "Executive"
#let ink = rgb("213834")
#let green = rgb("246954")
#let pale = rgb("eef4f0")
#let literal(value) = text(value.replace(regex("\\S{24,}"), m => m.text.clusters().join("\u{200b}")))
#set document(title: d.title, date: none)
#set text(font: "Noto Sans CJK TC", size: if executive { 8pt } else { 8.5pt }, fill: ink, lang: "en")
#set page(paper: "a4", margin: (top: 17mm, bottom: 18mm, x: 16mm),
  header: text(size: 7pt, fill: green)[GREENLENS / ENERGY / #text(d.kind)],
  footer: context [#text(size: 7pt)[#text(d.report_id) · #text(d.revision)] #h(1fr) #counter(page).display("1 / 1", both: true)])
#set par(leading: 0.45em)
#show heading.where(level: 1): it => block(above: 10pt, below: 5pt, text(size: if executive { 13pt } else { 16pt }, fill: green, weight: "bold", it))
#text(size: 22pt, weight: "bold", d.title)
#v(2mm)
#text(size: 11pt, fill: green)[#text(d.kind) · #text(d.revision)]
#v(2mm)
#block(fill: pale, inset: 7pt, width: 100%)[#text(d.scope)]
#v(2mm)
#text(size: 7pt)[Derived presentation SHA-256: #literal(d.dataset_sha256)]
#parbreak()
#text(size: 7pt)[Execution, integrity, currency and qualification are separate. Simulation metrics are not certification.]
#if not executive {
  v(3mm)
  outline(title: [Contents], depth: 1, indent: 8pt)
  pagebreak()
}
#for s in d.sections [
  #if executive and s.title == "Design and service" { pagebreak() }
  #heading(level: 1, text(s.title))
  #for p in s.paragraphs [#text(p) #parbreak()]
  #if s.image != none {
    // Names are produced by Rust, never imported attachment paths.
    assert(s.image in ("baseline-model.svg", "proposed-model.svg", "monthly.svg", "annual.svg") or s.image.match(regex("^schedule-(baseline|proposed)-[0-9]{3}\\.svg$")) != none)
    figure(image(s.image, width: 100%), caption: text(s.caption))
  }
  #if s.headers.len() > 0 and s.rows.len() == 0 { text("No records were supplied for this section.") }
  #if s.headers.len() > 0 and s.rows.len() > 0 {
    table(columns: if s.headers.len() == 2 { (34%, 66%) } else { s.headers.len() }, inset: if executive { 3pt } else { 4pt },
      stroke: 0.35pt + rgb("cbd9d1"),
      table.header(repeat: true, ..s.headers.map(h => text(weight: "bold", h))),
      ..s.rows.map(row => row.map(value => literal(value))).flatten())
  }
]
