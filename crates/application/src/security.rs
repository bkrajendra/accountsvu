//! Optional app lock (spec D14): a PIN or password asked for when the app
//! starts and after a period of inactivity.
//!
//! The lock protects against someone using an unattended computer; it does
//! not encrypt the database (full-disk encryption does that). The PIN and a
//! one-time recovery code are kept only as Argon2id hashes. Repeated wrong
//! attempts are slowed down.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::crypto::{self, KdfParams, SecretHash};
use crate::error::{AppError, AppResult, FieldError};

/// Allowed inactivity timeouts in minutes; 0 = only when the app starts or
/// when locked by hand.
pub const IDLE_CHOICES: [u32; 6] = [0, 1, 5, 15, 30, 60];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockConfig {
    pub version: u32,
    pub pin: SecretHash,
    pub recovery: SecretHash,
    pub idle_minutes: u32,
}

#[derive(Debug)]
pub struct NewLock {
    pub config: LockConfig,
    /// Shown once; only its hash is kept.
    pub recovery_code: String,
}

fn field(name: &str, msg: &str) -> AppError {
    AppError::Validation(vec![FieldError {
        field: name.into(),
        message: msg.into(),
    }])
}

/// A PIN of 4–12 digits, or a password of 6–64 characters.
pub fn validate_pin(pin: &str) -> AppResult<()> {
    let n = pin.chars().count();
    if pin.chars().all(|c| c.is_ascii_digit()) {
        if !(4..=12).contains(&n) {
            return Err(field(
                "pin",
                "Use 4 to 12 digits, or a password of at least 6 characters.",
            ));
        }
    } else if !(6..=64).contains(&n) {
        return Err(field(
            "pin",
            "A password needs 6 to 64 characters; a PIN needs 4 to 12 digits.",
        ));
    }
    Ok(())
}

pub fn validate_idle(minutes: u32) -> AppResult<()> {
    if IDLE_CHOICES.contains(&minutes) {
        Ok(())
    } else {
        Err(field("idleMinutes", "Choose one of the listed times."))
    }
}

/// Unambiguous characters (no 0/O, 1/I/L).
const CODE_CHARS: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

/// "7KQM-2XRP-H9TD-W4NA" — 16 random characters (~79 bits).
pub fn new_recovery_code() -> AppResult<String> {
    let raw = crypto::random_bytes::<16>().map_err(|e| AppError::Invalid(e.to_string()))?;
    let chars: Vec<char> = raw
        .iter()
        .map(|b| char::from(CODE_CHARS[usize::from(*b) % CODE_CHARS.len()]))
        .collect();
    Ok(chars
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-"))
}

/// Case, spaces and dashes don't matter when typing the recovery code.
pub fn normalise_recovery(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

pub fn new_lock(pin: &str, idle_minutes: u32) -> AppResult<NewLock> {
    new_lock_with(pin, idle_minutes, KdfParams::STANDARD)
}

pub(crate) fn new_lock_with(pin: &str, idle_minutes: u32, kdf: KdfParams) -> AppResult<NewLock> {
    validate_pin(pin)?;
    validate_idle(idle_minutes)?;
    let code = new_recovery_code()?;
    let err = |e: crypto::CryptoError| AppError::Invalid(e.to_string());
    Ok(NewLock {
        config: LockConfig {
            version: 1,
            pin: SecretHash::new(pin, kdf).map_err(err)?,
            recovery: SecretHash::new(&normalise_recovery(&code), kdf).map_err(err)?,
            idle_minutes,
        },
        recovery_code: code,
    })
}

impl LockConfig {
    pub fn check_pin(&self, pin: &str) -> bool {
        self.pin.verify(pin)
    }

    pub fn check_recovery(&self, code: &str) -> bool {
        self.recovery.verify(&normalise_recovery(code))
    }
}

/// Slows down guessing: after 5 wrong attempts each further one waits
/// 30 s, doubling up to 15 minutes. Resets on success.
#[derive(Debug, Default)]
pub struct Throttle {
    failures: u32,
    blocked_until: Option<Instant>,
}

impl Throttle {
    const FREE: u32 = 5;
    const MAX: Duration = Duration::from_secs(15 * 60);

    /// How long the caller must still wait before trying again.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.blocked_until
            .and_then(|t| t.checked_duration_since(now))
            .filter(|d| !d.is_zero())
    }

    pub fn failed(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        if self.failures >= Self::FREE {
            let steps = (self.failures - Self::FREE).min(5);
            let delay = Duration::from_secs(30)
                .saturating_mul(1 << steps)
                .min(Self::MAX);
            self.blocked_until = Some(now + delay);
        }
    }

    pub fn succeeded(&mut self) {
        *self = Throttle::default();
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::TEST_KDF;

    #[test]
    fn pins_and_passwords() {
        assert!(validate_pin("4821").is_ok());
        assert!(validate_pin("123").is_err());
        assert!(validate_pin("1234567890123").is_err());
        assert!(validate_pin("abc12").is_err());
        assert!(validate_pin("correct horse").is_ok());
        assert!(validate_idle(5).is_ok());
        assert!(validate_idle(7).is_err());
    }

    #[test]
    fn lock_checks_pin_and_recovery_code() {
        let l = new_lock_with("4821", 5, TEST_KDF).unwrap();
        assert_eq!(l.recovery_code.len(), 19);
        assert!(l.config.check_pin("4821"));
        assert!(!l.config.check_pin("4822"));
        let typed = l.recovery_code.to_lowercase().replace('-', " ");
        assert!(l.config.check_recovery(&typed));
        assert!(!l.config.check_recovery("AAAA-AAAA-AAAA-AAAA"));
        // Stored form never contains the secrets.
        let json = serde_json::to_string(&l.config).unwrap();
        assert!(!json.contains("4821") && !json.contains(&l.recovery_code));
    }

    #[test]
    fn throttle_slows_repeated_failures() {
        let t0 = Instant::now();
        let mut t = Throttle::default();
        for _ in 0..4 {
            t.failed(t0);
            assert_eq!(t.wait(t0), None);
        }
        t.failed(t0);
        assert_eq!(t.wait(t0), Some(Duration::from_secs(30)));
        t.failed(t0);
        assert_eq!(t.wait(t0), Some(Duration::from_secs(60)));
        assert_eq!(t.wait(t0 + Duration::from_secs(61)), None);
        for _ in 0..20 {
            t.failed(t0);
        }
        assert_eq!(t.wait(t0), Some(Duration::from_secs(15 * 60)));
        t.succeeded();
        assert_eq!(t.wait(t0), None);
        assert_eq!(t.failures(), 0);
    }
}
