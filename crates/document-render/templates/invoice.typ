// AccountsVU sales document. Reads every value from /data.json as plain
// strings, so nothing in the data is interpreted as markup.
// Layout follows the app branding: Inter, brand blue accents, white paper.

#let d = json("/data.json")

#let ink = rgb("#1d1d1f")
#let muted = rgb("#6e6e73")
#let rule = rgb("#e3e3e8")
#let brand = rgb("#0a6fd8")
#let wash = rgb("#f2f7fe")

#set document(title: d.title + " " + d.number, author: d.seller.name)
#set text(font: "Inter", size: 8.8pt, fill: ink, lang: "en", region: "IN")
#show table: set text(number-width: "tabular")
#set par(leading: 0.5em, spacing: 0.6em)
#set page(
  paper: "a4",
  margin: (x: 15mm, top: 15mm, bottom: 18mm),
  background: if d.watermark != none {
    rotate(-30deg, text(84pt, weight: 700, fill: rgb(29, 29, 31, 14), tracking: 4pt, d.watermark))
  },
  footer: context {
    set text(7.5pt, fill: muted)
    d.footer
    h(1fr)
    [Page #counter(page).display() of #counter(page).final().first()]
  },
)

#let label(body) = text(7.4pt, weight: 600, fill: muted, tracking: 0.6pt, upper(body))

#let party-block(p) = {
  text(10pt, weight: 600, p.name)
  if p.subtitle != none { linebreak(); text(fill: muted, p.subtitle) }
  for l in p.lines { linebreak(); text(fill: muted, l) }
}

// ---- Header: logo + seller on the left, document title on the right ----
#grid(
  columns: (1fr, auto),
  column-gutter: 12mm,
  grid(
    columns: if d.logo != none { (auto, 1fr) } else { (1fr,) },
    column-gutter: 4mm,
    ..if d.logo != none { (box(image(d.logo, height: 15mm, fit: "contain")),) },
    {
      text(13pt, weight: 600, d.seller.name)
      if d.seller.subtitle != none { linebreak(); text(fill: muted, d.seller.subtitle) }
      for l in d.seller.lines { linebreak(); text(fill: muted, l) }
    },
  ),
  align(right, {
    text(8pt, weight: 700, fill: brand, tracking: 1.2pt, upper(d.title))
    linebreak()
    v(1mm)
    text(14pt, weight: 600, d.number)
    linebreak()
    text(fill: muted, "Dated " + d.date)
    if d.due != none { linebreak(); text(fill: muted, "Due " + d.due) }
  }),
)

#v(5mm)
#line(length: 100%, stroke: 0.6pt + rule)
#v(4mm)

// ---- Bill to / place of supply ----
#grid(
  columns: (1fr, 1fr),
  column-gutter: 12mm,
  {
    label("Bill to")
    v(1.2mm)
    party-block(d.buyer)
  },
  align(right, {
    label("Place of supply")
    v(1.2mm)
    text(weight: 500, d.place)
    linebreak()
    text(fill: muted, d.supply)
    for f in d.flags { linebreak(); text(fill: muted, f) }
    if d.against != none { linebreak(); "Against " + d.against }
  }),
)

#v(4mm)

// ---- Lines ----
#table(
  columns: (auto, 1fr, auto, auto, auto, auto, auto),
  align: (left, left, left, right, right, right, right),
  stroke: none,
  inset: (x: 2.2mm, y: 2.1mm),
  fill: (_, y) => if y == 0 { wash },
  table.header(
    ..("#", "Description", "HSN/SAC", "Qty", "Rate", "GST", "Taxable").map(h => text(7.4pt, weight: 600, fill: brand, tracking: 0.4pt, upper(h))),
  ),
  ..for l in d.lines {
    (
      table.hline(stroke: 0.5pt + rule),
      text(fill: muted, l.no),
      {
        l.description
        if l.discount != none { linebreak(); text(7.6pt, fill: muted, l.discount) }
      },
      text(fill: muted, l.hsn),
      l.qty,
      l.rate,
      text(fill: muted, l.gst),
      l.taxable,
    )
  },
  table.hline(stroke: 0.8pt + rule),
)

#v(3mm)

// ---- Tax breakup (left) and totals (right) ----
#grid(
  columns: (1fr, 62mm),
  column-gutter: 10mm,
  if d.tax_rows.len() > 0 {
    // Only the taxes this document charges.
    let cols = (("Rate", r => r.rate), ("Taxable", r => r.taxable))
    if d.show_cgst { cols += (("CGST", r => r.cgst), (d.state_tax, r => r.sgst)) }
    if d.show_igst { cols += (("IGST", r => r.igst),) }
    if d.show_cess { cols += (("Cess", r => r.cess),) }
    table(
      columns: cols.len(),
      align: (left,) + (right,) * (cols.len() - 1),
      stroke: none,
      inset: (x: 1.6mm, y: 1.4mm),
      table.header(..cols.map(c => text(7pt, weight: 600, fill: muted, upper(c.at(0))))),
      table.hline(stroke: 0.5pt + rule),
      ..for r in d.tax_rows { cols.map(c => text(7.8pt, (c.at(1))(r))) },
    )
  },
  {
    for t in d.totals {
      grid(columns: (1fr, auto), text(fill: muted, t.at(0)), t.at(1))
      v(1.2mm)
    }
    line(length: 100%, stroke: 0.8pt + rule)
    v(1.5mm)
    grid(
      columns: (1fr, auto),
      align: horizon,
      text(10pt, weight: 600, "Total"),
      text(13pt, weight: 600, fill: brand, d.total),
    )
  },
)

#v(2mm)
#align(right, text(7.8pt, fill: muted, d.words))

#if d.reason != none {
  v(4mm)
  text(fill: muted, "Reason: ")
  d.reason
}

#if d.notes != none {
  v(4mm)
  label("Notes")
  v(1mm)
  text(fill: muted, d.notes)
}

// ---- Signature (kept whole, pushed to the foot of the last page) ----
#v(4mm)
#v(1fr)
#align(right, block(width: 62mm, breakable: false, {
  set align(center)
  text(weight: 500, d.sign_for)
  v(10mm)
  line(length: 100%, stroke: 0.5pt + rule)
  v(1mm)
  text(7.8pt, fill: muted, if d.signatory != none { d.signatory + " · Authorised signatory" } else { "Authorised signatory" })
}))
