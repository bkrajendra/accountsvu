// AccountsVU receipt, payment and refund voucher. Reads every value from
// /data.json as plain strings, so nothing in the data is interpreted as markup.
// Same look as the invoice: Inter, brand blue accents, white paper.

#let d = json("/data.json")

#let ink = rgb("#1d1d1f")
#let muted = rgb("#6e6e73")
#let rule = rgb("#e3e3e8")
#let brand = rgb("#0a6fd8")
#let wash = rgb("#f2f7fe")

#set document(title: d.title + " " + d.number, author: d.issuer.name)
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

// ---- Header: logo + business on the left, voucher title on the right ----
#grid(
  columns: (1fr, auto),
  column-gutter: 12mm,
  grid(
    columns: if d.logo != none { (auto, 1fr) } else { (1fr,) },
    column-gutter: 4mm,
    ..if d.logo != none { (box(image(d.logo, height: 15mm, fit: "contain")),) },
    {
      text(13pt, weight: 600, d.issuer.name)
      if d.issuer.subtitle != none { linebreak(); text(fill: muted, d.issuer.subtitle) }
      for l in d.issuer.lines { linebreak(); text(fill: muted, l) }
    },
  ),
  align(right, {
    text(8pt, weight: 700, fill: brand, tracking: 1.2pt, upper(d.title))
    linebreak()
    v(1mm)
    text(14pt, weight: 600, d.number)
    linebreak()
    text(fill: muted, "Dated " + d.date)
    if d.rule_note != none { linebreak(); text(7.6pt, fill: muted, d.rule_note) }
  }),
)

#v(5mm)
#line(length: 100%, stroke: 0.6pt + rule)
#v(4mm)

// ---- Counterparty (left) and payment details (right) ----
#grid(
  columns: (1fr, 1fr),
  column-gutter: 12mm,
  {
    label(d.counterparty_label)
    v(1.2mm)
    party-block(d.counterparty)
  },
  align(right, {
    for (i, kv) in d.details.enumerate() {
      if i > 0 { linebreak() }
      text(fill: muted, kv.at(0) + " ")
      text(weight: 500, kv.at(1))
    }
  }),
)

#v(5mm)

// ---- What the money was for ----
#table(
  columns: (1fr, auto),
  align: (left, right),
  stroke: none,
  inset: (x: 2.2mm, y: 2.1mm),
  fill: (_, y) => if y == 0 { wash },
  table.header(
    ..("Particulars", "Amount").map(h => text(7.4pt, weight: 600, fill: brand, tracking: 0.4pt, upper(h))),
  ),
  ..for r in d.rows {
    (
      table.hline(stroke: 0.5pt + rule),
      {
        r.description
        for x in r.details { linebreak(); text(7.6pt, fill: muted, x) }
      },
      r.amount,
    )
  },
  table.hline(stroke: 0.8pt + rule),
)

#v(2mm)
#grid(
  columns: (1fr, 62mm),
  column-gutter: 10mm,
  if d.tax != none {
    block(breakable: false, {
      label(d.tax.heading)
      v(1.2mm)
      text(weight: 500, "Place of supply " + d.tax.place)
      linebreak()
      text(fill: muted, d.tax.supply)
      v(1.5mm)
      for t in d.tax.rows {
        grid(columns: (1fr, auto), text(fill: muted, t.at(0)), t.at(1))
        v(0.8mm)
      }
      line(length: 100%, stroke: 0.5pt + rule)
      v(0.8mm)
      grid(columns: (1fr, auto), text(weight: 500, "Total tax"), text(weight: 500, d.tax.total))
    })
  },
  {
    grid(
      columns: (1fr, auto),
      align: horizon,
      text(10pt, weight: 600, "Total"),
      text(13pt, weight: 600, fill: brand, d.total),
    )
    v(1.5mm)
    align(right, text(7.8pt, fill: muted, d.words))
  },
)

#if d.narration != none {
  v(4mm)
  label("Narration")
  v(1mm)
  text(fill: muted, d.narration)
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
