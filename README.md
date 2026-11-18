# AccountsVU — Small Business Accounting & GST

Local-first desktop accounting for Indian micro and small businesses: invoicing, purchases, double-entry books and guided GST returns.

- **Spec:** [docs/accountsvu-spec.md](docs/accountsvu-spec.md) (v0.2, decisions in §12)
- **Plan and phase status:** [docs/implementation-plan.md](docs/implementation-plan.md)

## Stack

| Layer | Tech |
|---|---|
| Desktop shell | Tauri 2 (`src-tauri/`, thin IPC layer) |
| Domain and use cases | Rust workspace crates (`crates/`) |
| Storage | SQLite via SQLx (embedded migrations, runtime-checked queries) |
| UI | React 19 + TypeScript + Vite, Tailwind CSS v4, TanStack Query, React Router, Inter (bundled) |

## Layout

```text
Cargo.toml            workspace root
src-tauri/            Tauri host: commands → av-application
crates/
  domain/             Money, Gstin, Pan, StateCode, FinancialYear, series formats, masters (HSN/SAC, UQC)
  accounting/         balanced JournalEntry, reversals, default chart of accounts
  gst-engine/         CGST/SGST/UTGST/IGST split, document tax (inclusive prices, zero-rated), round-off, known rates
  persistence/        repository traits (ports), shared records, SQLite adapter + migrations
  application/        AppCore use cases (setup, masters, import, sales, purchases + ITC, attachments, posting) with RequestContext
src/                  React UI (components/, pages/, lib/, styles/)
docs/                 spec and implementation plan
```

## Develop

Prerequisites: Rust (stable), Node 20+, pnpm, and the WebView2 runtime (preinstalled on Windows 11).

```bash
pnpm install
```

```bash
pnpm tauri dev
```

The UI alone runs in a browser with preview data:

```bash
pnpm dev
```

Tests and lints:

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets
```

```bash
pnpm build
```

## Conventions

- Money never touches binary floating point. Rust uses `rust_decimal`, the database stores INTEGER paise, and amounts cross IPC as decimal strings. Clippy denies `float_arithmetic`.
- Posted journal lines are immutable, enforced both in Rust and by SQLite triggers. Corrections are made with reversal entries.
- The UI talks to Rust only through Tauri commands in `src-tauri/src/commands.rs`, which delegate to `av-application`.
- Use cases take a `RequestContext` and depend only on the repository traits in `av-persistence`; every query is scoped by `organisation_id`. Database-specific SQL lives in the adapter (`persistence/src/sqlite`).
- Theme tokens live in `src/styles/index.css`. Components use semantic colours (`bg-surface`, `text-muted`, `text-accent`), never raw hex values.
