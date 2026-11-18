//! Persistence ports and adapters (spec D9, D20).
//!
//! * [`repo`] — repository traits the application layer depends on.
//! * [`records`] — row types shared by every adapter.
//! * [`sqlite`] — the SQLite adapter used in personal mode.
//!   A `postgres` adapter implementing the same traits arrives in Phase 5.

pub mod records;
pub mod repo;
pub mod sqlite;

pub use repo::{
    AccountRepo, AttachmentRepo, AuditRepo, BankingRepo, DbResult, ItemRepo, JournalRepo,
    LedgerQueries, OrganisationRepo, PartyRepo, PurchaseRepo, RegistrationRepo, SalesRepo,
    SeriesRepo, Store, UnitOfWork,
};
pub use sqlite::SqliteStore;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("could not create data directory: {0}")]
    Io(#[from] std::io::Error),
}
