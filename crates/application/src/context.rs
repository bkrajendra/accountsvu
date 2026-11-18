//! Who is acting, on which organisation, with which role (spec D20, §4, §7.5).
//!
//! The transport builds the context: the Tauri host uses
//! [`RequestContext::local_owner`]; the hosted API (Phase 5) builds it from the
//! authenticated session.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Roles from spec §4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Accountant,
    Operator,
    Auditor,
}

/// What a use case needs to be allowed to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// Read business profile, masters, series and the chart of accounts.
    ViewBusiness,
    /// Create or change business setup, GSTINs and number series.
    ManageBusiness,
    /// Create or change parties and items (including opening balances).
    ManageMasters,
    /// Create, edit and delete draft sales and purchase documents (spec §4: operators too).
    PrepareDocuments,
    /// Issue, record, cancel or review documents — anything that posts to the books.
    PostDocuments,
    /// Read the audit log.
    ViewAudit,
}

impl Role {
    pub fn allows(self, p: Permission) -> bool {
        match p {
            Permission::ViewBusiness => true,
            Permission::ManageBusiness => self == Role::Owner,
            Permission::ManageMasters | Permission::PostDocuments => {
                matches!(self, Role::Owner | Role::Accountant)
            }
            Permission::PrepareDocuments => self != Role::Auditor,
            Permission::ViewAudit => matches!(self, Role::Owner | Role::Accountant | Role::Auditor),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Actor {
    /// Stable user id; written to the audit log.
    pub user_id: String,
    pub display_name: String,
    pub role: Role,
}

/// Which organisation a request acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrgScope {
    /// Personal mode: the single organisation on this install (created by
    /// first-run setup). Never used by the hosted API.
    Install,
    /// A specific organisation (tenant), e.g. from the authenticated session.
    Organisation(String),
}

#[derive(Clone, Debug)]
pub struct RequestContext {
    pub actor: Actor,
    pub scope: OrgScope,
}

impl RequestContext {
    /// Personal mode: one local user who owns the install's organisation (D14).
    pub fn local_owner() -> Self {
        RequestContext {
            actor: Actor {
                user_id: "owner".into(),
                display_name: "Owner".into(),
                role: Role::Owner,
            },
            scope: OrgScope::Install,
        }
    }

    /// Acting on a specific organisation (commercial mode, tests).
    pub fn for_organisation(actor: Actor, organisation_id: impl Into<String>) -> Self {
        RequestContext {
            actor,
            scope: OrgScope::Organisation(organisation_id.into()),
        }
    }

    pub fn require(&self, p: Permission) -> AppResult<()> {
        if self.actor.role.allows(p) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}
