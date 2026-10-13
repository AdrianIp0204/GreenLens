// Original fixed GreenLens energy report template. Apache-2.0.
// All displayed quantities, units and missing states are supplied by Rust.
// JSON strings are text, never evaluated Typst source or asset paths.
#let d = json("document.json")
#assert(d.schema_version == "greenlens.energy-document.v0.4")
#let ink = rgb("213834")
#let green = rgb("246954")
#let pale = rgb("eef4f0")
// Optional break positions preserve every character of long hashes/identifiers.
#let literal(value) = text(value.replace(regex("\\S{24,}"), m => m.text.clusters().join("\u{200b}")))
#set document(title: d.title, author: "GreenLens", date: none)
#set text(font: "Noto Sans CJK TC", size: 8.5pt, fill: ink, lang: "en")
#set page(paper: "a4", margin: (top: 18mm, bottom: 18mm, x: 16mm),
  header: text(size: 7pt, fill: green)[GREENLENS / ENERGY / #text(d.kind)],
  footer: context [#text(size: 7pt)[#text(d.report_id) · #text(d.revision)] #h(1fr) #counter(page).display("1 / 1", both: true)])
#set par(leading: 0.5em)
#show heading.where(level: 1): it => block(above: 13pt, below: 7pt, text(size: 17pt, fill: green, weight: "bold", it))
#show heading.where(level: 2): it => block(above: 9pt, below: 5pt, text(size: 11pt, fill: green, weight: "bold", it))
#text(size: 23pt, weight: "bold", d.title)
#v(4mm)
#text(size: 13pt, fill: green, d.kind)
#v(3mm)
#block(fill: pale, inset: 9pt, width: 100%)[#text(d.scope)]
#v(3mm)
#text(size: 7pt)[Dataset SHA-256: #literal(d.dataset_sha256)]
#v(5mm)
#text(size: 7pt)[Execution, evidence integrity, input currency and scientific qualification are separate. Simulation metrics are not certification.]
#v(3mm)
#outline(title: [Contents], depth: 1, indent: 8pt)
#pagebreak()
#for s in d.sections [
  #heading(level: 1, text(s.title))
  #for p in s.paragraphs [#text(p) #parbreak()]
  #if s.image != none {
    // These four names are compiled application assets, never imported paths.
    assert(s.image in ("baseline-model.svg", "proposed-model.svg", "monthly.svg", "annual.svg"))
    figure(image(s.image, width: 100%), caption: text(s.caption))
  }
  #if s.headers.len() > 0 and s.rows.len() == 0 {
    text("No records were supplied for this section.")
  }
  #if s.headers.len() > 0 and s.rows.len() > 0 {
    table(columns: if s.headers.len() == 2 { (38%, 62%) } else { s.headers.len() }, inset: 4pt,
      stroke: 0.35pt + rgb("cbd9d1"),
      table.header(repeat: true, ..s.headers.map(h => text(weight: "bold", h))),
      ..s.rows.map(row => row.map(value => literal(value))).flatten())
  }
]
