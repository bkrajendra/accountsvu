//! Default chart of accounts for an Indian small business (spec §5.1).
//!
//! Accounts with a `role` are system accounts: posting rules find them by
//! role, never by code or name, so users can rename or renumber freely.

use crate::AccountKind;
use AccountKind::*;

#[derive(Clone, Copy, Debug)]
pub struct DefaultAccount {
    pub code: &'static str,
    pub name: &'static str,
    pub kind: AccountKind,
    pub role: Option<&'static str>,
}

const fn acc(
    code: &'static str,
    name: &'static str,
    kind: AccountKind,
    role: Option<&'static str>,
) -> DefaultAccount {
    DefaultAccount {
        code,
        name,
        kind,
        role,
    }
}

pub const DEFAULT_CHART: &[DefaultAccount] = &[
    // Assets
    acc("1000", "Cash in hand", Asset, Some("cash")),
    acc("1010", "Bank account", Asset, Some("bank")),
    acc("1100", "Accounts receivable", Asset, Some("receivables")),
    acc("1200", "Input CGST", Asset, Some("input_cgst")),
    acc("1201", "Input SGST", Asset, Some("input_sgst")),
    acc("1202", "Input UTGST", Asset, Some("input_utgst")),
    acc("1203", "Input IGST", Asset, Some("input_igst")),
    acc("1204", "Input cess", Asset, Some("input_cess")),
    acc(
        "1210",
        "Input tax pending review",
        Asset,
        Some("itc_pending"),
    ),
    acc("1300", "Inventory", Asset, Some("inventory")),
    acc("1400", "Fixed assets", Asset, Some("fixed_assets")),
    acc("1500", "Advances and deposits", Asset, None),
    // Liabilities
    acc("2000", "Accounts payable", Liability, Some("payables")),
    acc("2100", "Output CGST", Liability, Some("output_cgst")),
    acc("2101", "Output SGST", Liability, Some("output_sgst")),
    acc("2102", "Output UTGST", Liability, Some("output_utgst")),
    acc("2103", "Output IGST", Liability, Some("output_igst")),
    acc("2104", "Output cess", Liability, Some("output_cess")),
    acc(
        "2110",
        "GST payable under reverse charge",
        Liability,
        Some("rcm_payable"),
    ),
    acc("2300", "Loans", Liability, None),
    // Equity
    acc("3000", "Owner's capital", Equity, Some("capital")),
    acc("3100", "Drawings", Equity, Some("drawings")),
    acc(
        "3200",
        "Retained earnings",
        Equity,
        Some("retained_earnings"),
    ),
    acc(
        "3900",
        "Opening balance adjustment",
        Equity,
        Some("opening_balance"),
    ),
    // Income
    acc("4000", "Sales", Income, Some("sales")),
    acc("4900", "Other income", Income, Some("other_income")),
    acc("4950", "Discount received", Income, None),
    // Expenses
    acc("5000", "Purchases", Expense, Some("purchases")),
    acc("5100", "Freight inward", Expense, None),
    acc("6000", "Rent", Expense, None),
    acc("6010", "Salaries and wages", Expense, None),
    acc("6020", "Electricity", Expense, None),
    acc("6030", "Telephone and internet", Expense, None),
    acc("6040", "Office expenses", Expense, None),
    acc("6050", "Professional fees", Expense, None),
    acc("6060", "Bank charges", Expense, Some("bank_charges")),
    acc("6070", "Travel and conveyance", Expense, None),
    acc("6080", "Repairs and maintenance", Expense, None),
    acc("6090", "Discount allowed", Expense, None),
    acc("6900", "Round-off", Expense, Some("round_off")),
    acc(
        "6950",
        "GST interest and late fee",
        Expense,
        Some("gst_interest"),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_and_roles_are_unique() {
        let codes: HashSet<_> = DEFAULT_CHART.iter().map(|a| a.code).collect();
        assert_eq!(codes.len(), DEFAULT_CHART.len());
        let roles: Vec<_> = DEFAULT_CHART.iter().filter_map(|a| a.role).collect();
        assert_eq!(roles.iter().collect::<HashSet<_>>().len(), roles.len());
    }

    #[test]
    fn posting_roles_exist() {
        let roles: HashSet<_> = DEFAULT_CHART.iter().filter_map(|a| a.role).collect();
        for r in [
            "cash",
            "bank",
            "receivables",
            "payables",
            "sales",
            "purchases",
            "round_off",
            "output_cgst",
            "output_sgst",
            "output_utgst",
            "output_igst",
            "output_cess",
            "input_cgst",
            "input_sgst",
            "input_utgst",
            "input_igst",
            "input_cess",
        ] {
            assert!(roles.contains(r), "missing role {r}");
        }
    }
}
