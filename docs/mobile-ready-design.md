# Mobile-ready design (future plan)

Status: **planned, not scheduled.** Written 2026-11-22 after the v0.2.5 review. Pick this up when a mobile, tablet or web build is on the roadmap, or earlier for steps 1–2 if small laptop windows need it.

## Where we are

- The desktop window has a minimum size of 1024 × 660 (`src-tauri/tauri.conf.json`), so the installed app never renders narrower than that today. The squeezed layout shows up in the browser preview and would show up in any future mobile or web build.
- Every screen was designed for desktop widths:
  - the shell has a fixed 232 px sidebar (64 px collapsed), `px-8` page padding and a top bar with no wrapping;
  - page headers put the title and action buttons side by side;
  - only three files use width breakpoints (`sm:`/`md:`/`lg:`): `Overview.tsx`, `Gstr2bPage.tsx` and `SettingsSection.tsx`;
  - about 21 files render tables and about 21 use multi-column grids.
- What goes wrong at phone width: text wraps one word per line, cards are squeezed thin, and the action buttons run off the right edge.

## Target widths

| Width | Device | Goal |
| --- | --- | --- |
| ≥ 1024 px | Desktop, laptop | Current design, unchanged |
| 768–1023 px | Tablet, small window | Icon sidebar, everything readable, no horizontal page scroll |
| < 768 px (check at 375 px) | Phone | Drawer navigation, stacked layouts, usable entry |

Use Tailwind's default breakpoints (`sm` 640, `md` 768, `lg` 1024). Don't add a parallel breakpoint system.

## Steps

Steps are ordered by payoff. Each one can ship on its own.

### 1. Shell — about half a day

Files: `src/components/AppShell.tsx`, `Sidebar.tsx`, `TopBar.tsx`.

- **Below `lg`:** the sidebar is forced to its icon-only state, whatever the saved preference.
- **Below `md`:** the sidebar is hidden.
  - A menu button in the top bar opens it as a slide-in drawer with a scrim.
  - It closes on navigation, on Escape and on a scrim tap, and keeps focus inside while open.
- **Top bar:**
  - search collapses to an icon button that opens a full-width search field;
  - the GSTIN / financial-year switcher becomes a compact chip (financial year only, GSTIN in its menu).
- **Page padding:** `px-4` on phones, `px-6` on tablets, `px-8` from `lg`.
- **Keyboard shortcuts:** Ctrl+K and Ctrl+B keep working. Ctrl+B toggles the drawer when the sidebar is hidden.

### 2. Page headers and card grids — about half a day

- Shared header pattern (title, subtitle, actions in `ui.tsx`):
  - title and actions stack below `md`;
  - actions wrap;
  - the primary action stays first.
- Stat tiles (Overview, GST pages, Reports): 4 columns → 2 at `md` → 1 on phones.
- Setup and empty-state cards: the icon stacks above the text below `sm`, and the button goes full width.
- This is mostly mechanical class edits; there's no logic change.

### 3. List tables — about a day

- **Everywhere:** wrap each table in a horizontal-scroll container. The first column (document number or name) stays sticky, so the page itself never scrolls sideways.
- **Main lists on phones** (Sales, Purchases, Parties, Items): show one card per row with the number, party, date, amount and status. A tap opens the document as it does today.
- **Report and return tables** (Ledgers, Statements, Return centre, GSTR-1/2B/3B): keep the scroll container. These are reference views, and their column layout carries meaning.

### 4. Dialogs and forms — about half a day

- `Dialog.tsx`: below `md`, dialogs become full-screen sheets with a sticky header and footer.
- `form.tsx` and each dialog's grids: 2–3 column forms drop to one column below `md`.
- Comboboxes and date fields need a usable touch target height (at least 40 px).

### 5. Invoice, bill and voucher editors — 1–2 days

Only needed if documents will be created on phones.

- The line-item grid (item, HSN, qty, rate, discount, tax, amount) can't fit at 375 px.
- On phones, each line becomes a summary card. Tapping it opens an "edit line" sheet with the full fields. An "Add line" button sits under the list.
- Totals and the tax breakdown move to a sticky bottom summary with a Save button.
- Files: `src/components/editor.tsx`, `pages/sales/*Editor*`, `pages/purchases/PurchaseEditor.tsx`, `pages/banking/VoucherEditor.tsx`.

## Estimate

| Scope | Effort | Result |
| --- | --- | --- |
| Steps 1–2 | ~1 day | Fine on any desktop or laptop window, readable on tablets |
| Steps 1–4 | ~2.5 days | Usable on phones for viewing and simple entry |
| Steps 1–5 | ~4 days | Fully usable on phones, including creating invoices |

## Rules while doing it

- Desktop layout at ≥ 1024 px must not change. Diff screenshots before and after.
- No horizontal page scroll at any width. Only table containers may scroll sideways.
- Hover-only affordances need a tap equivalent.
- Drawers and sheets respect `prefers-reduced-motion`.
- Verify each step in the browser preview at 375, 768 and 1024 px, in light and dark themes.
- If a mobile build ships, lower or drop the window `minWidth` in `tauri.conf.json` for that target only.
