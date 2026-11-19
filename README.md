<div align="center">

<img src="docs/brand-kit/app-icon-1024.png" alt="AccountsVU logo" width="120" />

# AccountsVU

**Small business accounting & GST, on your own computer.**

Local-first desktop accounting for Indian micro and small businesses: invoicing, purchases, double-entry books and guided GST returns.

[![Release build](https://github.com/bkrajendra/accountsvu/actions/workflows/release.yml/badge.svg?branch=main)](https://github.com/bkrajendra/accountsvu/actions/workflows/release.yml)
[![Latest release](https://img.shields.io/github/v/release/bkrajendra/accountsvu?label=release&color=0A84FF)](https://github.com/bkrajendra/accountsvu/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/bkrajendra/accountsvu/total?color=0A84FF)](https://github.com/bkrajendra/accountsvu/releases)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)](https://github.com/bkrajendra/accountsvu/releases/latest)
<br />
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![React 19](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)](https://react.dev)
[![SQLite](https://img.shields.io/badge/SQLite-local-003B57?logo=sqlite&logoColor=white)](https://www.sqlite.org)

[Download](https://github.com/bkrajendra/accountsvu/releases/latest) · [Spec](docs/accountsvu-spec.md) · [Roadmap](docs/implementation-plan.md)

<img src="docs/screenshots/overview.webp" alt="AccountsVU overview screen" width="900" />

</div>

## Features

- **Sales and purchases:** GST invoices, credit and debit notes, bills with input tax credit, and print-ready PDF invoices with your logo.
- **Double-entry books:** balanced journals, immutable posted entries, reversals, cash and bank books, bank statement import and reconciliation.
- **GST built in:** CGST/SGST/UTGST/IGST split, place of supply, reverse charge and rate-wise tax breakup; guided GSTR-1 and GSTR-3B returns are next on the roadmap.
- **Reports:** trial balance, profit and loss, balance sheet, general ledger, party statements and receivables/payables ageing.
- **Your data stays with you:** a local SQLite database, encrypted backups (AES-256-GCM, Argon2id) with verified restore, and an optional PIN lock.

## Install

Download the installer for your system from the [latest release](https://github.com/bkrajendra/accountsvu/releases/latest): `.msi` / `.exe` for Windows, `.dmg` for macOS (Apple silicon or Intel), and `.AppImage` / `.deb` / `.rpm` for Linux.

> The macOS build is not notarised yet. On first launch, open **System Settings › Privacy & Security** and click **Open Anyway**.

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
