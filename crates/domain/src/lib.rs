//! AccountsVU domain value objects.
//!
//! Everything here is pure (no I/O) and safe to use from any layer.

pub mod business;
pub mod gstin;
pub mod ids;
pub mod masters;
pub mod money;
pub mod pan;
pub mod period;
pub mod series;
pub mod state;

pub use business::{Constitution, DocumentType, FilingFrequency, RegistrationType};
pub use gstin::{Gstin, GstinError};
pub use ids::{Id, new_id};
pub use masters::{ItemKind, PartyGstType, PartyKind, TaxCategory};
pub use money::{Money, MoneyError};
pub use pan::{Pan, PanError};
pub use period::FinancialYear;
pub use series::{SeriesError, SeriesFormat};
pub use state::StateCode;
