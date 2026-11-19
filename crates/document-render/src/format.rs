//! Money and number formatting for printed documents. Integer-only (paise),
//! Indian digit grouping, matching the app's on-screen formatting.

/// Paise → "₹1,23,456.78"; negative amounts get a leading minus sign.
pub fn rupees(paise: i64) -> String {
    let sign = if paise < 0 { "−" } else { "" };
    format!("{sign}₹{}", plain(paise.unsigned_abs()))
}

/// Paise (unsigned) → "1,23,456.78" without the currency sign.
pub fn plain(paise: u64) -> String {
    let whole = paise / 100;
    let frac = paise % 100;
    format!("{}.{frac:02}", group_indian(whole))
}

/// 1234567 → "12,34,567" (last three digits, then pairs).
pub fn group_indian(n: u64) -> String {
    let s = n.to_string();
    if s.len() <= 3 {
        return s;
    }
    let (head, tail) = s.split_at(s.len() - 3);
    let mut parts: Vec<&str> = Vec::new();
    let mut rest = head;
    while rest.len() > 2 {
        let (a, b) = rest.split_at(rest.len() - 2);
        parts.push(b);
        rest = a;
    }
    if !rest.is_empty() {
        parts.push(rest);
    }
    parts.reverse();
    format!("{},{tail}", parts.join(","))
}

/// Basis points → "18%", "0.25%", "12.5%".
pub fn rate(bp: i64) -> String {
    let whole = bp / 100;
    let frac = bp % 100;
    if frac == 0 {
        format!("{whole}%")
    } else if frac % 10 == 0 {
        format!("{whole}.{}%", frac / 10)
    } else {
        format!("{whole}.{frac:02}%")
    }
}

const ONES: [&str; 20] = [
    "",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

fn below_hundred(n: u64) -> String {
    let n = usize::try_from(n).unwrap_or(0);
    if n < 20 {
        ONES[n].to_string()
    } else if n % 10 == 0 {
        TENS[n / 10].to_string()
    } else {
        format!("{}-{}", TENS[n / 10], ONES[n % 10])
    }
}

fn below_thousand(n: u64) -> String {
    let h = n / 100;
    let r = n % 100;
    match (h, r) {
        (0, _) => below_hundred(r),
        (_, 0) => format!("{} hundred", below_hundred(h)),
        _ => format!("{} hundred {}", below_hundred(h), below_hundred(r)),
    }
}

/// Indian-system words: 1,23,45,678 → "one crore twenty-three lakh forty-five thousand six hundred seventy-eight".
pub fn integer_words(n: u64) -> String {
    if n == 0 {
        return "zero".into();
    }
    let crore = n / 10_000_000;
    let lakh = (n / 100_000) % 100;
    let thousand = (n / 1000) % 100;
    let rest = n % 1000;
    let mut parts = Vec::new();
    if crore > 0 {
        parts.push(format!("{} crore", integer_words(crore)));
    }
    if lakh > 0 {
        parts.push(format!("{} lakh", below_hundred(lakh)));
    }
    if thousand > 0 {
        parts.push(format!("{} thousand", below_hundred(thousand)));
    }
    if rest > 0 {
        parts.push(below_thousand(rest));
    }
    parts.join(" ")
}

/// "Rupees one thousand two hundred and fifty paise only".
pub fn amount_in_words(paise: i64) -> String {
    let abs = paise.unsigned_abs();
    let rupees = abs / 100;
    let p = abs % 100;
    let mut s = format!("Rupees {}", integer_words(rupees));
    if p > 0 {
        s.push_str(&format!(" and {} paise", below_hundred(p)));
    }
    s.push_str(" only");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_like_india() {
        assert_eq!(rupees(0), "₹0.00");
        assert_eq!(rupees(99), "₹0.99");
        assert_eq!(rupees(123_456), "₹1,234.56");
        assert_eq!(rupees(12_345_678_901), "₹12,34,56,789.01");
        assert_eq!(rupees(-40), "−₹0.40");
        assert_eq!(group_indian(100_000), "1,00,000");
    }

    #[test]
    fn rates() {
        assert_eq!(rate(1800), "18%");
        assert_eq!(rate(25), "0.25%");
        assert_eq!(rate(1250), "12.5%");
        assert_eq!(rate(0), "0%");
    }

    #[test]
    fn words() {
        assert_eq!(amount_in_words(0), "Rupees zero only");
        assert_eq!(
            amount_in_words(118_000),
            "Rupees one thousand one hundred eighty only"
        );
        assert_eq!(
            amount_in_words(1_234_567_850),
            "Rupees one crore twenty-three lakh forty-five thousand six hundred seventy-eight and fifty paise only"
        );
    }
}
