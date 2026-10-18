// Original GreenLens paired analytical report. Apache-2.0.
// Fixed renderer/1 for the separate design-report contract; no old issue changes.
#let d = json("document.json")
#assert(d.schema_version == "greenlens.design-report-document.v1")
#let executive = d.kind == "Design executive"
#let literal(value) = text(value.replace(regex("\\S{24,}"), m => m.text.clusters().join("\u{200b}")))
#set document(title: d.title, author: "GreenLens", date: none)
#set text(font: "Noto Sans CJK TC", size: 8.5pt, fill: rgb("213834"), lang: "en")
#set page(paper: "a4", margin: (top: 17mm, bottom: 18mm, x: 16mm),
  header: text(size: 7pt)[GREENLENS / DESIGN COMPARISON / #text(d.kind)],
  footer: context [#text(size: 7pt)[#text(d.revision)] #h(1fr) #counter(page).display("1 / 1", both: true)])
#set par(leading: 0.5em)
#show heading.where(level: 1): it => block(above: 10pt, below: 6pt, text(size: 15pt, fill: rgb("246954"), weight: "bold", it))
#text(size: 22pt, weight: "bold", d.title)
#v(2mm)
#text(size: 12pt, d.kind)
#v(3mm)
#block(fill: rgb("eef4f0"), inset: 8pt, width: 100%)[#text(d.scope)]
#v(2mm)
#text(size: 7pt)[Content SHA-256: #literal(d.dataset_sha256)]
#v(3mm)
#text(size: 7pt)[Geometry drawings are not spatial simulation fields. Execution, integrity, input relationship, numerical sensitivity and scientific qualification are separate. No certification or professional approval is inferred.]
#v(3mm)
#if not executive { outline(title: [Contents], depth: 1, indent: 8pt) }
#for s in d.sections [
  #heading(level: 1, text(s.title))
  #for p in s.paragraphs [#text(p) #parbreak()]
  #if s.image != none {
    // Input JSON and images are produced by owned Rust code. The renderer never
    // accepts imported template source or user-selected resource paths.
    assert(not s.image.contains("/") and not s.image.contains("\\") and s.image.ends-with(".svg"))
    figure(image(s.image, width: 100%), caption: text(s.caption))
  }
  #if s.headers.len() > 0 and s.rows.len() == 0 { text("No records in this section.") }
  #if s.headers.len() > 0 and s.rows.len() > 0 {
    table(columns: if s.headers.len() == 2 { (32%, 68%) } else { s.headers.len() }, inset: 4pt,
      stroke: 0.35pt + rgb("cbd9d1"),
      table.header(repeat: true, ..s.headers.map(h => text(weight: "bold", h))),
      ..s.rows.map(row => row.map(value => literal(value))).flatten())
  }
]
