//! Manual tax override (spec §5.6, Phase 2.6). An owner or accountant may
//! enter a line's tax by hand, with a reason, when the calculated figure
//! differs from what was agreed or what the supplier charged. The calculated
//! tax is kept beside it, and who did it and when is stored and audited.

use av_domain::Money;
use av_gst_engine::{EnteredTax, LineTax, SupplyKind, override_line_tax};
use serde::Deserialize;

use crate::context::{Permission, RequestContext};
use crate::error::Errors;
use crate::masters::parse_amount;
use crate::setup::{check_len, clean};

/// Tax heads typed for one line, in rupees. `sgst` is SGST or UTGST.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxOverrideInput {
    #[serde(default)]
    pub cgst: Option<String>,
    #[serde(default)]
    pub sgst: Option<String>,
    #[serde(default)]
    pub igst: Option<String>,
    #[serde(default)]
    pub cess: Option<String>,
}

/// The tax to keep for a line, and the calculated tax when it was changed.
pub(crate) fn apply(
    e: &mut Errors,
    line: usize,
    input: Option<&TaxOverrideInput>,
    computed: LineTax,
    kind: SupplyKind,
) -> (LineTax, Option<i64>) {
    let Some(o) = input else {
        return (computed, None);
    };
    let f = |name: &str| format!("lines.{line}.taxOverride.{name}");
    let mut le = Errors::new("");
    let mut head = |name: &str, v: &Option<String>| {
        Money::from_paise(parse_amount(&mut le, &f(name), v, false).unwrap_or(0))
    };
    let entered = EnteredTax {
        cgst: head("cgst", &o.cgst),
        state: head("sgst", &o.sgst),
        igst: head("igst", &o.igst),
        cess: head("cess", &o.cess),
    };
    if !le.is_empty() {
        e.absorb(le);
        return (computed, None);
    }
    match override_line_tax(computed, kind, entered) {
        Ok(t) if t == computed => (computed, None),
        Ok(t) => (t, computed.total_tax().to_paise().ok()),
        Err(err) => {
            e.add(&format!("lines.{line}.taxOverride"), err.to_string());
            (computed, None)
        }
    }
}

/// Who changed the tax, why and when: `(reason, by, at)`. Adds errors when
/// lines were changed without a reason or by someone not allowed to.
pub(crate) fn stamp(
    e: &mut Errors,
    ctx: &RequestContext,
    any_changed: bool,
    reason: &Option<String>,
    now: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    if !any_changed {
        return (None, None, None);
    }
    if !ctx.actor.role.allows(Permission::PostDocuments) {
        e.add(
            "taxOverrideReason",
            "Only an owner or accountant can enter tax by hand.",
        );
    }
    let reason = clean(reason);
    if reason.is_none() {
        e.add("taxOverrideReason", "Say why the tax was entered by hand.");
    }
    check_len(e, "taxOverrideReason", &reason, 300);
    (
        reason,
        Some(ctx.actor.display_name.clone()),
        Some(now.to_string()),
    )
}
