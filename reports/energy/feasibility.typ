// Original GreenLens renderer fixture, Apache-2.0. No packages or evaluated user source.
#let data = json("report-data.json")
#assert(data.schema_version == "greenlens.energy-renderer-fixture.v1")
#let ink = rgb("213834")
#let accent = rgb("246954")
#let pale = rgb("eef4f0")
#set document(title: data.title, author: "GreenLens", date: none)
#set text(font: "Noto Sans CJK TC", size: 9.5pt, fill: ink, lang: "en")
#set page(paper: "a4", margin: (top: 19mm, bottom: 20mm, x: 20mm),
  header: text(size: 8pt, fill: accent)[GREENLENS / ENERGY / RENDERER FEASIBILITY],
  footer: context [#text(size: 8pt)[Synthetic display fixture · Not simulation evidence] #h(1fr) #counter(page).display("1 / 1", both: true)])
#set par(justify: false, leading: 0.6em)
#set heading(numbering: "1.")
#show heading.where(level: 1): it => block(above: 14pt, below: 9pt, text(size: 17pt, weight: "bold", fill: accent, it))
#let cell(value) = text(value)
#let band(body) = block(width: 100%, fill: pale, inset: 10pt, radius: 3pt, body)
#let styled-table(columns, ..cells) = table(columns: columns, inset: 6pt,
  stroke: 0.4pt + rgb("cbd9d1"),
  fill: (x, y) => if y == 0 { pale } else { none }, ..cells)

#v(16mm)
#text(size: 11pt, fill: accent, weight: "bold")[GATE 0 / ORIGINAL SYNTHETIC EXAMPLE]
#v(6mm)
#text(size: 30pt, weight: "bold", data.title)
#v(3mm)
#text(size: 16pt, data.cjk)
#v(8mm)
#band[
  *Rendering evidence only.* No EnergyPlus simulation was run for this fixture.
  Values below exercise units, missing states, pagination and literal text. They
  do not establish building performance, scientific qualification or certification.
]
#v(8mm)
#styled-table((35%, 65%),
  table.header([*Identity*], [*Retained value*]),
  [Report], cell(data.report_id), [Model], cell(data.model_id),
  [Baseline run], cell(data.baseline_run), [Proposed run], cell(data.proposed_run),
  [Revision], cell(data.revision), [Time convention], cell(data.time_convention))
#v(6mm)
#figure(image("model.svg", width: 100%), caption: [Original schematic four-zone view. Illustrative geometry only; not a thermal model.])
#pagebreak()
= Scope and comparison
#band(text(data.scope))
#styled-table((27%, 73%), table.header([*Assessment axis*], [*Status*]),
  ..data.assessments.map(a => (cell(a.axis), cell(a.status))).flatten())

== Performance table
All entries are preformatted fixture strings. The renderer performs no energy,
savings, demand or carbon arithmetic. A missing factor suppresses emissions;
an unsupported system does not become a zero.
#figure(
 styled-table((30%, 16%, 16%, 16%, 22%),
  table.header([*End use*], [*Baseline*], [*Proposed*], [*Unit*], [*State / note*]),
  ..data.performance.map(r => (cell(r.label), cell(r.baseline), cell(r.proposed), cell(r.unit), cell(r.note))).flatten()),
 caption: [Synthetic typed-state and signed-value display. Values are not measured or simulated.])
#figure(image("chart.svg", width: 90%), caption: [Original display-only chart. Its labels and values are included in report-data.json.])
Demand is a named interval quantity. A future report must bind each design's
coincident facility peak to its own timestamp; independent end-use maxima must
never be added to invent a facility peak.
#pagebreak()
= Input inventory
This deliberately long table tests repeated column headings, wrapping and page
breaks. All #data.inputs.len() rows must survive PDF generation and text extraction.
The complete table is retained in the structured JSON fixture.
#styled-table((14%, 28%, 20%, 38%),
 table.header(repeat: true, [*Object ID*], [*Parameter*], [*Value / unit*], [*Source and scope*]),
 ..data.inputs.map(r => (cell(r.id), cell(r.parameter), cell(r.value), cell(r.source))).flatten())
#pagebreak()
= Literal text and limitations
Strings loaded from JSON are displayed as text. They are never parsed by
`eval`, used as source files, imports, asset paths or commands.
#for s in data.adversarial [
 #block(width: 100%, fill: pale, inset: 8pt)[#text(s)]
 #v(3pt)
]
== Evidence index
#styled-table((28%, 72%), table.header([*Item*], [*Role*]),
 [report-data.json], [Original renderer fixture; no simulation claim],
 [model.svg / chart.svg], [Original local vector assets],
 [feasibility.typ], [Owned fixed template, no external imports],
 [PDF and page PNGs], [Real pinned Typst outputs],
 [Renderer and font seals], [Separate reviewed installation; no system fonts])
== Remaining work
Production EnergyReportData, standalone HTML, both delivered report families,
native application integration, maximum-case report sizing and report archive
validation remain separate implementation and acceptance work. This fixture
does not satisfy those gates.

#text(size: 8pt)[END OF ORIGINAL RENDERER FIXTURE]
