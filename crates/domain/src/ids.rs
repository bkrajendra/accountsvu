//! Stable identifiers. UUID v7 keeps ids time-ordered (index friendly) and
//! globally unique, so local data can later sync to hosted mode (spec D2).

pub type Id = uuid::Uuid;

/// Create a new time-ordered identifier.
pub fn new_id() -> Id {
    uuid::Uuid::now_v7()
}
