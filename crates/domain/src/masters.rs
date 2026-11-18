//! Masters: parties (customers/suppliers) and items (spec §5.1, §5.4).

use crate::str_enum;

str_enum!(
    /// Whether we sell to, buy from, or both.
    PartyKind {
        Customer => "customer",
        Supplier => "supplier",
        Both => "both",
    }
);

impl PartyKind {
    pub fn is_customer(self) -> bool {
        matches!(self, PartyKind::Customer | PartyKind::Both)
    }
    pub fn is_supplier(self) -> bool {
        matches!(self, PartyKind::Supplier | PartyKind::Both)
    }
}

str_enum!(
    /// The party's GST status. Drives B2B/B2C classification and whether a
    /// GSTIN is required.
    PartyGstType {
        Regular => "regular",
        Composition => "composition",
        Unregistered => "unregistered",
        Consumer => "consumer",
        Overseas => "overseas",
        Sez => "sez",
    }
);

impl PartyGstType {
    /// A GSTIN is mandatory for these types.
    pub fn requires_gstin(self) -> bool {
        matches!(
            self,
            PartyGstType::Regular | PartyGstType::Composition | PartyGstType::Sez
        )
    }
    /// These types must not carry a GSTIN.
    pub fn forbids_gstin(self) -> bool {
        matches!(
            self,
            PartyGstType::Unregistered | PartyGstType::Consumer | PartyGstType::Overseas
        )
    }
}

str_enum!(
    ItemKind {
        Goods => "goods",
        Service => "service",
    }
);

str_enum!(
    /// How an item is treated for GST. Exempt, nil-rated and non-GST supplies
    /// are reported separately and never carry tax.
    TaxCategory {
        Taxable => "taxable",
        Exempt => "exempt",
        NilRated => "nil_rated",
        NonGst => "non_gst",
    }
);

/// GST Unique Quantity Codes (UQC) accepted by the portal for HSN summaries.
pub const UQC: &[(&str, &str)] = &[
    ("BAG", "Bags"),
    ("BAL", "Bale"),
    ("BDL", "Bundles"),
    ("BKL", "Buckles"),
    ("BOU", "Billion of units"),
    ("BOX", "Box"),
    ("BTL", "Bottles"),
    ("BUN", "Bunches"),
    ("CAN", "Cans"),
    ("CBM", "Cubic metres"),
    ("CCM", "Cubic centimetres"),
    ("CMS", "Centimetres"),
    ("CTN", "Cartons"),
    ("DOZ", "Dozens"),
    ("DRM", "Drums"),
    ("GGK", "Great gross"),
    ("GMS", "Grammes"),
    ("GRS", "Gross"),
    ("GYD", "Gross yards"),
    ("KGS", "Kilograms"),
    ("KLR", "Kilolitre"),
    ("KME", "Kilometre"),
    ("LTR", "Litres"),
    ("MLT", "Millilitre"),
    ("MTR", "Metres"),
    ("MTS", "Metric ton"),
    ("NOS", "Numbers"),
    ("OTH", "Others"),
    ("PAC", "Packs"),
    ("PCS", "Pieces"),
    ("PRS", "Pairs"),
    ("QTL", "Quintal"),
    ("ROL", "Rolls"),
    ("SET", "Sets"),
    ("SQF", "Square feet"),
    ("SQM", "Square metres"),
    ("SQY", "Square yards"),
    ("TBS", "Tablets"),
    ("TGM", "Ten gross"),
    ("THD", "Thousands"),
    ("TON", "Tonnes"),
    ("TUB", "Tubes"),
    ("UGS", "US gallons"),
    ("UNT", "Units"),
    ("YDS", "Yards"),
];

pub fn is_valid_uqc(code: &str) -> bool {
    UQC.iter().any(|(c, _)| *c == code)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HsnSacError {
    #[error("use digits only")]
    NotDigits,
    #[error("an HSN code has 4, 6 or 8 digits")]
    HsnLength,
    #[error("HSN codes don't start with 99 — that chapter is for services (SAC)")]
    HsnIsService,
    #[error("a SAC code has 6 digits and starts with 99")]
    Sac,
}

/// Validate an HSN (goods) or SAC (services) code's shape. Whether the code
/// exists is checked against the rule pack's list in Phase 2.
pub fn validate_hsn_sac(code: &str, kind: ItemKind) -> Result<(), HsnSacError> {
    if code.is_empty() || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(HsnSacError::NotDigits);
    }
    match kind {
        ItemKind::Goods => {
            if code.starts_with("99") {
                Err(HsnSacError::HsnIsService)
            } else if matches!(code.len(), 4 | 6 | 8) {
                Ok(())
            } else {
                Err(HsnSacError::HsnLength)
            }
        }
        ItemKind::Service => {
            if code.len() == 6 && code.starts_with("99") {
                Ok(())
            } else {
                Err(HsnSacError::Sac)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsn_and_sac_shapes() {
        assert_eq!(validate_hsn_sac("8471", ItemKind::Goods), Ok(()));
        assert_eq!(validate_hsn_sac("84713010", ItemKind::Goods), Ok(()));
        assert_eq!(
            validate_hsn_sac("84713", ItemKind::Goods),
            Err(HsnSacError::HsnLength)
        );
        assert_eq!(
            validate_hsn_sac("998314", ItemKind::Goods),
            Err(HsnSacError::HsnIsService)
        );
        assert_eq!(validate_hsn_sac("998314", ItemKind::Service), Ok(()));
        assert_eq!(
            validate_hsn_sac("8471", ItemKind::Service),
            Err(HsnSacError::Sac)
        );
        assert_eq!(
            validate_hsn_sac("84A1", ItemKind::Goods),
            Err(HsnSacError::NotDigits)
        );
    }

    #[test]
    fn party_gst_types() {
        assert!(PartyGstType::Regular.requires_gstin());
        assert!(PartyGstType::Consumer.forbids_gstin());
        assert_eq!(PartyGstType::parse("sez"), Some(PartyGstType::Sez));
        assert!(is_valid_uqc("NOS") && !is_valid_uqc("XYZ"));
    }
}
