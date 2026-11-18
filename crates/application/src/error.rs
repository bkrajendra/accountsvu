use av_persistence::DbError;
use serde::Serialize;

/// A validation message tied to a form field path such as
/// `organisation.legalName`, so the UI can show it inline (spec §6.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("please correct the highlighted fields")]
    Validation(Vec<FieldError>),
    #[error("{0}")]
    Conflict(String),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("the business is already set up")]
    AlreadySetUp,
    #[error("set up your business first")]
    NotSetUp,
    #[error("you don't have permission to do this")]
    Forbidden,
}

impl AppError {
    /// Stable machine-readable code for the UI (spec §7.5).
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Db(_) => "database_error",
            AppError::Invalid(_) => "invalid_input",
            AppError::Validation(_) => "validation_failed",
            AppError::Conflict(_) => "conflict",
            AppError::NotFound(_) => "not_found",
            AppError::AlreadySetUp => "already_set_up",
            AppError::NotSetUp => "not_set_up",
            AppError::Forbidden => "forbidden",
        }
    }

    pub fn field_errors(&self) -> &[FieldError] {
        match self {
            AppError::Validation(f) => f,
            _ => &[],
        }
    }

    pub(crate) fn stale() -> Self {
        AppError::Conflict("This record was changed elsewhere. Reload it and try again.".into())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Collects field errors; `prefix` namespaces nested forms.
pub(crate) struct Errors {
    prefix: String,
    list: Vec<FieldError>,
}

impl Errors {
    pub fn new(prefix: &str) -> Self {
        Errors {
            prefix: prefix.to_owned(),
            list: Vec::new(),
        }
    }

    pub fn add(&mut self, field: &str, message: impl Into<String>) {
        self.list.push(FieldError {
            field: format!("{}{}", self.prefix, field),
            message: message.into(),
        });
    }

    pub fn absorb(&mut self, other: Errors) {
        self.list.extend(other.list);
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn finish(self) -> AppResult<()> {
        if self.list.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(self.list))
        }
    }
}
