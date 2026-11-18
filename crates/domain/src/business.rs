//! Business-setup enums and small validated values (spec §5.1).

#[macro_export]
#[doc(hidden)]
macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $s),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some($name::$variant),)+ _ => None }
            }
        }
    };
}

str_enum!(
    /// Legal constitution of the business.
    Constitution {
        Proprietorship => "proprietorship",
        Partnership => "partnership",
        Llp => "llp",
        PrivateLimited => "private_limited",
        PublicLimited => "public_limited",
        Huf => "huf",
        Trust => "trust",
        Society => "society",
        Other => "other",
    }
);

impl Constitution {
    /// PAN holder-type letters that are consistent with this constitution.
    /// `None` means any letter is acceptable.
    pub fn expected_pan_types(self) -> Option<&'static [char]> {
        match self {
            Constitution::Proprietorship => Some(&['P']),
            Constitution::Partnership | Constitution::Llp => Some(&['F']),
            Constitution::PrivateLimited | Constitution::PublicLimited => Some(&['C']),
            Constitution::Huf => Some(&['H']),
            Constitution::Trust => Some(&['T', 'A']),
            Constitution::Society => Some(&['A', 'T']),
            Constitution::Other => None,
        }
    }
}

str_enum!(
    /// GST registration type. MVP targets regular taxpayers (spec §2.1).
    RegistrationType {
        Regular => "regular",
        Composition => "composition",
        Sez => "sez_unit",
        Casual => "casual",
    }
);

str_enum!(
    FilingFrequency {
        Monthly => "monthly",
        Quarterly => "quarterly",
    }
);

str_enum!(
    /// Documents that carry their own number series.
    DocumentType {
        TaxInvoice => "tax_invoice",
        BillOfSupply => "bill_of_supply",
        ExportInvoice => "export_invoice",
        CreditNote => "credit_note",
        DebitNote => "debit_note",
        ReceiptVoucher => "receipt_voucher",
        PaymentVoucher => "payment_voucher",
        Estimate => "estimate",
    }
);

impl DocumentType {
    pub fn default_prefix(self) -> &'static str {
        match self {
            DocumentType::TaxInvoice => "INV",
            DocumentType::BillOfSupply => "BOS",
            DocumentType::ExportInvoice => "EXP",
            DocumentType::CreditNote => "CN",
            DocumentType::DebitNote => "DN",
            DocumentType::ReceiptVoucher => "RV",
            DocumentType::PaymentVoucher => "PV",
            DocumentType::Estimate => "EST",
        }
    }

    /// Whether the GST rules on document numbering (Rule 46: ≤16 chars,
    /// unique per FY) apply. Estimates are not GST documents.
    pub fn is_gst_document(self) -> bool {
        !matches!(self, DocumentType::Estimate)
    }
}

/// Indian PIN code: 6 digits, not starting with 0.
pub fn is_valid_pincode(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 6 && b.iter().all(u8::is_ascii_digit) && b[0] != b'0'
}

/// Lenient phone check: 8–15 digits, allowing `+`, spaces and hyphens.
pub fn is_valid_phone(s: &str) -> bool {
    let digits = s.chars().filter(char::is_ascii_digit).count();
    (8..=15).contains(&digits)
        && s.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '-'))
}

/// Minimal structural e-mail check; real verification is out of scope.
pub fn is_valid_email(s: &str) -> bool {
    match s.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !s.contains(char::is_whitespace)
                && !domain.contains('@')
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_round_trip() {
        for c in Constitution::ALL {
            assert_eq!(Constitution::parse(c.as_str()), Some(*c));
        }
        assert_eq!(
            DocumentType::parse("credit_note"),
            Some(DocumentType::CreditNote)
        );
        assert_eq!(FilingFrequency::parse("weekly"), None);
    }

    #[test]
    fn contacts() {
        assert!(is_valid_pincode("411001"));
        assert!(!is_valid_pincode("011001"));
        assert!(!is_valid_pincode("41100"));
        assert!(is_valid_phone("+91 98200-12345"));
        assert!(!is_valid_phone("12345"));
        assert!(is_valid_email("accounts@example.in"));
        assert!(!is_valid_email("accounts@example"));
        assert!(!is_valid_email("a b@example.in"));
    }
}
