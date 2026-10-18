//! Parent PIN (spec §8): hashing, lockout after repeated wrong PINs, idle-expiring sessions.
//! `Auth` takes `now` as an argument so the timing rules are testable without sleeping.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const SESSION_IDLE: Duration = Duration::from_secs(10 * 60);
pub const MAX_FAILURES: u32 = 5;
pub const LOCKOUT: Duration = Duration::from_secs(60);

/// A PIN is 4 to 8 digits.
pub fn valid_pin(pin: &str) -> bool {
    (4..=8).contains(&pin.len()) && pin.bytes().all(|b| b.is_ascii_digit())
}

pub fn hash_pin(pin: &str) -> String {
    let salt =
        SaltString::encode_b64(&rand::random::<[u8; 16]>()).expect("16 bytes is a valid salt");
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .expect("argon2 hashing with default parameters cannot fail")
        .to_string()
}

pub fn verify_pin(pin: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| {
        Argon2::default()
            .verify_password(pin.as_bytes(), &h)
            .is_ok()
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Login {
    /// New session token.
    Ok(String),
    WrongPin,
    Locked {
        retry_after: Duration,
    },
}

#[derive(Default)]
pub struct Auth {
    /// token → last activity
    sessions: HashMap<String, Instant>,
    failures: u32,
    locked_until: Option<Instant>,
}

impl Auth {
    /// Tries a login. `pin_ok` runs the (slow) PIN check and is skipped while locked.
    pub fn login(&mut self, now: Instant, pin_ok: impl FnOnce() -> bool) -> Login {
        if let Some(until) = self.locked_until {
            if now < until {
                return Login::Locked {
                    retry_after: until - now,
                };
            }
            self.locked_until = None;
            self.failures = 0;
        }
        if !pin_ok() {
            self.failures += 1;
            if self.failures >= MAX_FAILURES {
                self.locked_until = Some(now + LOCKOUT);
                return Login::Locked {
                    retry_after: LOCKOUT,
                };
            }
            return Login::WrongPin;
        }
        self.failures = 0;
        let token: String = rand::random::<[u8; 32]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.sessions.insert(token.clone(), now);
        Login::Ok(token)
    }

    /// True if `token` is a session active within the last 10 minutes; refreshes it.
    pub fn touch(&mut self, token: &str, now: Instant) -> bool {
        self.sessions
            .retain(|_, last| now.duration_since(*last) < SESSION_IDLE);
        match self.sessions.get_mut(token) {
            Some(last) => {
                *last = now;
                true
            }
            None => false,
        }
    }

    pub fn logout(&mut self, token: &str) {
        self.sessions.remove(token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mins(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    fn token(login: Login) -> String {
        match login {
            Login::Ok(t) => t,
            other => panic!("expected a session, got {other:?}"),
        }
    }

    #[test]
    fn pin_is_four_to_eight_digits() {
        for ok in ["1234", "12345678", "0000"] {
            assert!(valid_pin(ok), "{ok}");
        }
        for bad in ["", "123", "123456789", "12a4", "12 4", "١٢٣٤"] {
            assert!(!valid_pin(bad), "{bad}");
        }
    }

    #[test]
    fn hashed_pin_verifies_only_the_right_pin() {
        let hash = hash_pin("2468");
        assert_ne!(hash, "2468");
        assert!(verify_pin("2468", &hash));
        assert!(!verify_pin("2469", &hash));
        assert!(!verify_pin("2468", "not a hash"));
    }

    #[test]
    fn right_pin_opens_a_session() {
        let mut auth = Auth::default();
        let t0 = Instant::now();
        let t = token(auth.login(t0, || true));
        assert!(auth.touch(&t, t0));
        assert!(!auth.touch("someone-else", t0));
    }

    #[test]
    fn fifth_wrong_pin_locks_for_a_minute() {
        let mut auth = Auth::default();
        let t0 = Instant::now();
        for _ in 0..4 {
            assert_eq!(auth.login(t0, || false), Login::WrongPin);
        }
        assert_eq!(
            auth.login(t0, || false),
            Login::Locked {
                retry_after: LOCKOUT
            }
        );
        let during = auth.login(t0 + Duration::from_secs(20), || {
            panic!("PIN must not be checked while locked")
        });
        assert_eq!(
            during,
            Login::Locked {
                retry_after: Duration::from_secs(40)
            }
        );
        token(auth.login(t0 + LOCKOUT, || true));
    }

    #[test]
    fn right_pin_resets_the_failure_count() {
        let mut auth = Auth::default();
        let t0 = Instant::now();
        for _ in 0..4 {
            auth.login(t0, || false);
        }
        token(auth.login(t0, || true));
        for _ in 0..4 {
            assert_eq!(auth.login(t0, || false), Login::WrongPin);
        }
    }

    #[test]
    fn session_expires_after_ten_idle_minutes() {
        let mut auth = Auth::default();
        let t0 = Instant::now();
        let t = token(auth.login(t0, || true));
        assert!(auth.touch(&t, t0 + mins(9)));
        assert!(auth.touch(&t, t0 + mins(18))); // activity keeps it alive
        assert!(!auth.touch(&t, t0 + mins(28)));
        assert!(!auth.touch(&t, t0 + mins(18))); // gone for good
    }

    #[test]
    fn logout_ends_the_session() {
        let mut auth = Auth::default();
        let t0 = Instant::now();
        let t = token(auth.login(t0, || true));
        auth.logout(&t);
        assert!(!auth.touch(&t, t0));
    }
}
