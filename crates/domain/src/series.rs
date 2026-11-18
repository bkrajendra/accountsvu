//! Document number series (spec §5.1).
//!
//! GST Rule 46 requires invoice numbers to be unique within a financial year,
//! at most 16 characters, using only letters, digits, `-` and `/`.

use serde::{Deserialize, Serialize};

pub const MAX_GST_DOC_NUMBER_LEN: usize = 16;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SeriesError {
    #[error("prefix and suffix may only contain letters, digits, '-' and '/'")]
    Charset,
    #[error("document numbers would be {len} characters; GST allows at most 16")]
    TooLong { len: usize },
    #[error("number padding must be between 1 and 8 digits")]
    Padding,
    #[error("next number must be at least 1")]
    NextNumber,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesFormat {
    pub prefix: String,
    pub suffix: String,
    /// Minimum digits; numbers are zero-padded to this width.
    pub padding: u8,
}

impl SeriesFormat {
    /// Normalise (trim, uppercase) and validate the format. With `gst_limits`,
    /// the prefix + padded number + suffix must fit in 16 characters.
    pub fn new(
        prefix: &str,
        suffix: &str,
        padding: u8,
        gst_limits: bool,
    ) -> Result<Self, SeriesError> {
        let f = SeriesFormat {
            prefix: prefix.trim().to_ascii_uppercase(),
            suffix: suffix.trim().to_ascii_uppercase(),
            padding,
        };
        if !(1..=8).contains(&padding) {
            return Err(SeriesError::Padding);
        }
        let ok = |s: &str| {
            s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/')
        };
        if !ok(&f.prefix) || !ok(&f.suffix) {
            return Err(SeriesError::Charset);
        }
        if gst_limits {
            let len = f.prefix.len() + f.suffix.len() + padding as usize;
            if len > MAX_GST_DOC_NUMBER_LEN {
                return Err(SeriesError::TooLong { len });
            }
        }
        Ok(f)
    }

    pub fn format(&self, number: u64) -> String {
        format!(
            "{}{:0width$}{}",
            self.prefix,
            number,
            self.suffix,
            width = self.padding as usize
        )
    }

    /// Whether `number` still produces a GST-compliant length.
    pub fn fits(&self, number: u64) -> bool {
        self.format(number).len() <= MAX_GST_DOC_NUMBER_LEN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_with_padding() {
        let f = SeriesFormat::new("inv/26-27/", "", 4, true).unwrap();
        assert_eq!(f.prefix, "INV/26-27/");
        assert_eq!(f.format(7), "INV/26-27/0007");
        assert_eq!(f.format(12345), "INV/26-27/12345");
        assert!(f.fits(999_999));
        assert!(!f.fits(10_000_000));
    }

    #[test]
    fn enforces_gst_rules() {
        assert_eq!(
            SeriesFormat::new("INV 1", "", 4, true),
            Err(SeriesError::Charset)
        );
        assert_eq!(
            SeriesFormat::new("INVOICE/2026-27/", "", 4, true),
            Err(SeriesError::TooLong { len: 20 })
        );
        assert!(SeriesFormat::new("INVOICE/2026-27/", "", 4, false).is_ok());
        assert_eq!(
            SeriesFormat::new("A", "", 0, true),
            Err(SeriesError::Padding)
        );
    }
}
