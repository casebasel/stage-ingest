// Kopierbericht von Stage Ingest. Daten kommen als JSON in sys.inputs.daten (bericht/src/lib.rs).
// Druckfassung der App-Welt: Geist für Wörter, Geist Mono für Werte, Farbe nur für das Urteil.
#let d = json(bytes(sys.inputs.daten))

#let ok = rgb("#2f8a46")
#let gold = rgb("#b07a10")
#let rot = rgb("#b3261e")  // nur Datenverlust: nicht freigegeben (gemeinsames Design)
#let leise = rgb("#5d6470")
#let linie = rgb("#d5d8dd")

#set document(title: "Kopierbericht " + d.karte, author: "Stage Ingest")
#set page(
  paper: "a4",
  margin: (x: 18mm, top: 20mm, bottom: 18mm),
  footer: context [
    #set text(8pt, fill: leise)
    Stage Ingest #d.version · #d.rechner · Zurückgelesen am Zwischenspeicher des Betriebssystems vorbei
    #h(1fr) Seite #counter(page).display() von #counter(page).final().first()
  ],
)
#set text(font: "Geist", size: 9.5pt, lang: "de")
#show raw: set text(font: "Geist Mono")
#let mono(x) = text(font: "Geist Mono", x)

#text(9pt, fill: leise)[Kopierbericht]
#v(-6pt)
#text(20pt, weight: "semibold")[#d.karte]
#v(2pt)
#text(fill: leise)[#d.quelle]

#v(10pt)
#block(stroke: (left: 3pt + if d.freigabe.sicher { ok } else { rot }), inset: (left: 10pt, y: 4pt))[
  #text(14pt, weight: "semibold", fill: if d.freigabe.sicher { ok } else { rot })[
    #if d.freigabe.sicher [Sicher zum Formatieren] else [Nicht freigegeben · Karte nicht formatieren]
  ] \
  #d.freigabe.grund (verlangt: #d.freigabe.mindest_kopien)
  #for h in d.freigabe.hinweise [ \ #text(fill: gold)[#h] ]
]

#v(8pt)
#grid(
  columns: (auto, 1fr),
  column-gutter: 14pt,
  row-gutter: 5pt,
  ..d.projekt.map(((n, w)) => (text(fill: leise)[#n], [#w])).flatten(),
  text(fill: leise)[Beginn], mono(d.beginn),
  text(fill: leise)[Ende], mono(d.ende),
  text(fill: leise)[Dateien], [#mono(str(d.dateien.len())) · #mono(d.summe)],
  text(fill: leise)[Prüfsumme], mono(if d.mit_md5 { "XXH3-128 + MD5" } else { "XXH3-128" }),
  text(fill: leise)[Dieser Bericht], [liegt auf Ziel #d.dieses_ziel],
)

== Ziele
#v(4pt)
#table(
  columns: (auto, 1fr, auto, auto),
  stroke: (x, y) => (bottom: 0.5pt + linie),
  inset: (x: 4pt, y: 5pt),
  table.header(..([Nr.], [Ordner und Gerät], [Geprüft], [Ergebnis]).map(x => text(fill: leise, x))),
  ..d.ziele.enumerate().map(((i, z)) => (
    mono(str(i + 1)),
    [#mono(z.ordner) \ #text(fill: leise)[#z.geraet]],
    mono(str(z.geprueft)),
    if z.gut { text(fill: ok)[gut] } else { text(fill: gold)[fehlerhaft] },
  )).flatten(),
)
#for z in d.ziele.filter(z => not z.gut) [
  #text(fill: gold, weight: "semibold")[#z.ordner] \
  #for f in z.fehler [ #text(fill: gold)[– #f] \ ]
]

#let bilder = sys.inputs.at("bilder", default: ())
#if bilder.len() > 0 [
  == Clips
  #v(4pt)
  #set text(8pt)
  #table(
    columns: (auto, 1fr),
    stroke: (x, y) => (bottom: 0.4pt + linie),
    inset: (x: 3pt, y: 4pt),
    align: (x, y) => horizon + left,
    ..bilder.map(c => (
      [#mono(c.name) #if c.werte != "" [ \ #text(fill: leise)[#c.werte]]],
      if c.bilder.len() > 0 { stack(dir: ltr, spacing: 3pt, ..c.bilder.map(b => image(b, width: 38mm))) } else { [] },
    )).flatten(),
  )
]

== Dateien
#v(4pt)
#set text(8pt)
#table(
  columns: if d.mit_md5 { (1fr, auto, auto, auto) } else { (1fr, auto, auto) },
  stroke: (x, y) => (bottom: 0.4pt + linie),
  inset: (x: 3pt, y: 3.5pt),
  align: (x, y) => if x == 0 { left } else { right },
  table.header(..(([Datei], [Grösse], [XXH3-128]) + if d.mit_md5 { ([MD5],) } else { () }).map(x => text(fill: leise, x))),
  ..d.dateien.map(f => (mono(f.pfad), mono(f.groesse), mono(f.xxh128)) + if d.mit_md5 { (mono(f.md5),) } else { () }).flatten(),
)
