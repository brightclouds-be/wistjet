# Plan 2: App Shell, First Run & Parent PIN — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A double-clickable `wistjet` binary that serves the embedded web app, walks a parent through first-run setup, shows the profile screen, and guards the parent area with a server-enforced PIN.

**Architecture:** `src/auth.rs` holds PIN hashing, lockout and sessions as plain logic with an injected `now`. `src/family.rs` creates the family in one transaction. `src/server.rs` is the `axum` router: JSON API, a session guard on `/api/parent/*`, and the Svelte build embedded with `rust-embed`. `src/main.rs` resolves config, opens the database, binds `0.0.0.0` so tablets can connect, prints the addresses and opens the browser. The web app (`web/`) is a Svelte 5 SPA with hash routing. Plan 1's engine and store are untouched.

**Tech Stack:** Rust: `axum` 0.8, `tokio` 1, `serde`/`serde_json` 1, `argon2` 0.5, `rust-embed` 8 (`mime-guess`), `qrcode` 0.14 (`svg`), `directories` 6, `open` 5, `local-ip-address` 0.6; tests: `tower` 0.5, `http-body-util` 0.1. Web: Svelte 5.57, Vite 8, TypeScript 6, svelte-check 4. Node 22 at build time only.

**Spec:** `docs/superpowers/specs/2026-10-08-family-flashcards-design.md` (§6.1, §7 intro, §7.8, §8, §9.1). Roadmap: `docs/superpowers/plans/2026-10-11-wistjet-roadmap.md`.

## Decisions made while planning

The spec is silent on these; each is the smallest choice that fits it.

- **PIN format:** 4 to 8 digits.
- **Avatar:** a colour from a fixed palette of 8, stored as `#rrggbb`. The spec allows "avatar or colour".
- **Setup is one request** at the end of the wizard, so a half-finished setup can never be saved.
- **Tray icon and hidden console window move to Plan 5** (distribution). For now the console window is the "Wistjet is running" indicator; closing it stops the app. A tray icon pulls in GUI event-loop and Linux GTK build dependencies that belong with packaging.
- **Web screens have no unit tests here.** They are thin over the API, which is fully tested. Plan 2 checks them with `svelte-check`, a production build and a manual smoke run; Plan 5 adds the Playwright end-to-end test.
- **The profile tiles are not clickable yet.** Plan 3 builds the child flow behind them.

## Global Constraints

- All interface text is Dutch.
- No external network resources (CDNs, web fonts): the app must work offline on a home network.
- The server enforces the PIN for every parent action. Without a parent session a client can only read status, list children and (Plan 3) study.
- PIN stored hashed (Argon2). After 5 wrong attempts, PIN entry is blocked for 1 minute. Parent sessions expire after 10 minutes of inactivity.
- Names (family, child) are 1–50 characters after trimming.
- Parent session cookie: `HttpOnly; SameSite=Strict; Path=/`. No `Secure` flag, because the app runs over plain HTTP on a home network.
- All data lives in one folder: `WISTJET_DATA_DIR`, else the OS per-user data folder.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `npm --prefix web run check` all clean.

## Review Focus

Inputs the spec implies but doesn't spell out. Each has a check in the task named.

1. **A child guessing PINs:** after 5 wrong tries even the right PIN is refused for a minute. Test `wrong_pin_is_rejected_then_locked_out` (Task 3), `fifth_wrong_pin_locks_for_a_minute` (Task 1).
2. **A forged or foreign cookie:** must not open the parent area. Test `parent_routes_need_a_session` (Task 3).
3. **Setup finished twice** (two tabs, double submit): the second is refused and nothing is duplicated. Tests `setup_runs_only_once` (Task 2), `setup_twice_is_a_conflict` (Task 3).
4. **Names with accents or emoji:** limits count characters, not bytes, like SQLite's `length()`. Test `names_count_characters_not_bytes` (Task 2).
5. **Double-clicking the app twice:** the second start opens the browser on the running instance instead of failing. Smoke script step in Task 5.

## File Structure

| File | Responsibility |
|---|---|
| `src/auth.rs` | PIN format, Argon2 hash/verify, lockout, idle-expiring sessions |
| `src/family.rs` | First-run setup transaction, sample deck, family/child lookups |
| `src/server.rs` | `axum` router, handlers, parent guard, embedded static files |
| `src/main.rs` | Launcher: config from env, data folder, bind, banner, open browser |
| `tests/api.rs` | HTTP-level tests through the router |
| `web/` | Svelte SPA: `App.svelte` (hash routing), `routes/Setup.svelte`, `routes/Profiles.svelte`, `routes/Parent.svelte`, `lib/api.ts`, `app.css` |
| `.github/workflows/ci.yml` | Adds the web check and build before the Rust steps |

API added in this plan (all JSON):

| Method & path | Guard | Result |
|---|---|---|
| `GET /api/status` | none | `{ setup_done, lan_url }` |
| `POST /api/setup` | none, once | 204; 409 `already_set_up`; 400 `{ error: "invalid", field }` |
| `GET /api/children` | none | `[{ id, name, avatar }]` (active only) |
| `GET /api/lan-qr.svg` | none | QR code for `lan_url` |
| `POST /api/parent/login` | none | 204 + cookie; 401 `wrong_pin`; 429 `{ error: "locked", retry_after }`; 409 `not_set_up` |
| `POST /api/parent/logout` | none | 204, clears cookie |
| `GET /api/parent/session` | parent | 204; 401 `locked` |

Plan 4 adds its routes to the guarded `parent` router in `server::router`.

---

### Task 1: Parent PIN, lockout and sessions

**Files:**
- Create: `src/auth.rs`
- Modify: `Cargo.toml`, `src/lib.rs`

**Interfaces:**
- Produces: `auth::valid_pin(&str) -> bool`, `auth::hash_pin(&str) -> String`, `auth::verify_pin(pin: &str, hash: &str) -> bool`, `auth::Login { Ok(String), WrongPin, Locked { retry_after: Duration } }`, `auth::Auth` (`Default`) with `login(&mut self, now: Instant, pin_ok: impl FnOnce() -> bool) -> Login`, `touch(&mut self, token: &str, now: Instant) -> bool`, `logout(&mut self, token: &str)`; constants `SESSION_IDLE` (10 min), `MAX_FAILURES` (5), `LOCKOUT` (60 s)

- [ ] **Step 1: Add the dependency and module**

In `Cargo.toml`, add to `[dependencies]` (keep alphabetical order):
```toml
argon2 = "0.5"
```
and append at the end of the file:
```toml
# Argon2 is unusably slow unoptimised; keep PIN tests fast in debug builds.
[profile.dev.package.argon2]
opt-level = 3
[profile.dev.package.blake2]
opt-level = 3
```
In `src/lib.rs`, add as the first line:
```rust
pub mod auth;
```

- [ ] **Step 2: Write the failing tests**

Create `src/auth.rs` with the doc comment and the test module:
```rust
//! Parent PIN (spec §8): hashing, lockout after repeated wrong PINs, idle-expiring sessions.
//! `Auth` takes `now` as an argument so the timing rules are testable without sleeping.

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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test auth::`
Expected: compile errors, `cannot find function valid_pin`, `cannot find type Auth`, etc.

- [ ] **Step 4: Implement**

Insert between the doc comment and `#[cfg(test)]` in `src/auth.rs`:
```rust
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
```

The salt comes from `rand` (already a dependency; its thread RNG is cryptographically secure), which avoids enabling `argon2`'s `getrandom` feature. `login` takes the PIN check as a closure so it is skipped entirely while locked, and runs under the `Auth` mutex in Task 3, so concurrent guesses are serialised.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test auth:: && cargo clippy --all-targets -- -D warnings`
Expected: 7 tests pass; clippy clean.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/auth.rs
git commit -m "feat: parent PIN hashing, lockout and sessions"
```

---

### Task 2: First-run family setup

**Files:**
- Create: `src/family.rs`
- Modify: `Cargo.toml`, `src/lib.rs`

**Interfaces:**
- Consumes: `auth::valid_pin`, `auth::hash_pin` (Task 1); `db::open_in_memory`, `store::due_items` (Plan 1)
- Produces: `family::Setup { family_name, timezone, pin, child_name, child_avatar: String, sample_deck: bool }` (`Deserialize`), `family::SetupError { AlreadyDone, Invalid(&'static str), Db(rusqlite::Error) }`, `family::Child { id: i64, name: String, avatar: String }` (`Serialize`), `family::create(&mut Connection, &Setup) -> Result<(), SetupError>`, `family::is_set_up(&Connection) -> rusqlite::Result<bool>`, `family::pin_hash(&Connection) -> rusqlite::Result<Option<String>>`, `family::children(&Connection) -> rusqlite::Result<Vec<Child>>`, `family::clean_name(&str) -> Option<String>`, `family::valid_avatar(&str) -> bool` (Plan 4 reuses the last two)

- [ ] **Step 1: Add the dependency and module**

In `Cargo.toml`, add to `[dependencies]`:
```toml
serde = { version = "1", features = ["derive"] }
```
In `src/lib.rs`, add after `pub mod engine;`:
```rust
pub mod family;
```

- [ ] **Step 2: Write the failing tests**

Create `src/family.rs` with the doc comment and the test module:
```rust
//! The one family per installation: first-run setup (spec §7.8) and the lookups other code needs.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, store};

    fn setup(sample_deck: bool) -> Setup {
        Setup {
            family_name: "  De Smet  ".into(),
            timezone: "Europe/Brussels".into(),
            pin: "2468".into(),
            child_name: "Lotte".into(),
            child_avatar: "#e57373".into(),
            sample_deck,
        }
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn creates_family_and_first_child() {
        let mut conn = db::open_in_memory().unwrap();
        assert!(!is_set_up(&conn).unwrap());
        create(&mut conn, &setup(false)).unwrap();
        assert!(is_set_up(&conn).unwrap());
        let name: String = conn
            .query_row("SELECT name FROM family", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "De Smet");
        assert_eq!(
            children(&conn).unwrap(),
            vec![Child {
                id: 1,
                name: "Lotte".into(),
                avatar: "#e57373".into()
            }]
        );
        assert_eq!(count(&conn, "SELECT count(*) FROM deck"), 0);
    }

    #[test]
    fn pin_is_stored_hashed() {
        let mut conn = db::open_in_memory().unwrap();
        create(&mut conn, &setup(false)).unwrap();
        let hash = pin_hash(&conn).unwrap().unwrap();
        assert_ne!(hash, "2468");
        assert!(auth::verify_pin("2468", &hash));
    }

    #[test]
    fn sample_deck_is_assigned_and_due_today() {
        let mut conn = db::open_in_memory().unwrap();
        create(&mut conn, &setup(true)).unwrap();
        let today = "2026-10-17".parse().unwrap();
        assert_eq!(store::due_items(&conn, 1, None, today).unwrap().len(), 10);
    }

    #[test]
    fn setup_runs_only_once() {
        let mut conn = db::open_in_memory().unwrap();
        create(&mut conn, &setup(true)).unwrap();
        assert!(matches!(
            create(&mut conn, &setup(true)),
            Err(SetupError::AlreadyDone)
        ));
        assert_eq!(count(&conn, "SELECT count(*) FROM child"), 1);
    }

    #[test]
    fn invalid_input_names_the_field_and_writes_nothing() {
        type Break = fn(&mut Setup);
        let cases: [(&str, Break); 7] = [
            ("family_name", |s| s.family_name = "   ".into()),
            ("family_name", |s| s.family_name = "x".repeat(51)),
            ("timezone", |s| s.timezone = "Mars/Olympus".into()),
            ("pin", |s| s.pin = "12".into()),
            ("child_name", |s| s.child_name = "".into()),
            ("child_avatar", |s| s.child_avatar = "red".into()),
            ("child_avatar", |s| s.child_avatar = "#E57373".into()),
        ];
        for (field, break_it) in cases {
            let mut conn = db::open_in_memory().unwrap();
            let mut s = setup(true);
            break_it(&mut s);
            match create(&mut conn, &s) {
                Err(SetupError::Invalid(f)) => assert_eq!(f, field),
                other => panic!("{field}: expected Invalid, got {other:?}"),
            }
            assert!(!is_set_up(&conn).unwrap());
        }
    }

    #[test]
    fn names_count_characters_not_bytes() {
        assert!(clean_name(&"é".repeat(50)).is_some());
        assert!(clean_name(&"é".repeat(51)).is_none());
    }

    #[test]
    fn archived_children_are_not_listed() {
        let mut conn = db::open_in_memory().unwrap();
        create(&mut conn, &setup(false)).unwrap();
        conn.execute_batch("UPDATE child SET archived = 1").unwrap();
        assert!(children(&conn).unwrap().is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test family::`
Expected: compile errors, `cannot find struct Setup`, `cannot find function create`, etc.

- [ ] **Step 4: Implement**

Insert between the doc comment and `#[cfg(test)]` in `src/family.rs`:
```rust
use crate::auth;
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

/// What the first-run wizard submits, in one request so setup is all-or-nothing.
#[derive(Deserialize)]
pub struct Setup {
    pub family_name: String,
    pub timezone: String,
    pub pin: String,
    pub child_name: String,
    pub child_avatar: String,
    pub sample_deck: bool,
}

#[derive(Debug)]
pub enum SetupError {
    AlreadyDone,
    /// Names the offending field, as sent by the client.
    Invalid(&'static str),
    Db(rusqlite::Error),
}

impl From<rusqlite::Error> for SetupError {
    fn from(e: rusqlite::Error) -> Self {
        SetupError::Db(e)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Child {
    pub id: i64,
    pub name: String,
    pub avatar: String,
}

const SAMPLE_CARDS: [(&str, &str); 10] = [
    ("le chat", "de kat"),
    ("le chien", "de hond"),
    ("la maison", "het huis"),
    ("le livre", "het boek"),
    ("l'école", "de school"),
    ("la pomme", "de appel"),
    ("le pain", "het brood"),
    ("l'eau", "het water"),
    ("le soleil", "de zon"),
    ("la voiture", "de auto"),
];

/// Trimmed name of 1 to 50 characters (spec §8).
pub fn clean_name(s: &str) -> Option<String> {
    let s = s.trim();
    (1..=50).contains(&s.chars().count()).then(|| s.to_string())
}

/// An avatar is a colour: `#` and six lowercase hex digits.
pub fn valid_avatar(s: &str) -> bool {
    s.len() == 7
        && s.starts_with('#')
        && s[1..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn is_set_up(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM family)", [], |r| r.get(0))
}

pub fn pin_hash(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT pin_hash FROM family", [], |r| r.get(0))
        .optional()
}

/// Active children for the profile screen, in the order they were added.
pub fn children(conn: &Connection) -> rusqlite::Result<Vec<Child>> {
    let mut stmt =
        conn.prepare("SELECT id, name, avatar FROM child WHERE archived = 0 ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok(Child {
            id: r.get(0)?,
            name: r.get(1)?,
            avatar: r.get(2)?,
        })
    })?;
    rows.collect()
}

/// Creates the family, its first child and optionally the sample deck, in one transaction.
pub fn create(conn: &mut Connection, s: &Setup) -> Result<(), SetupError> {
    let family_name = clean_name(&s.family_name).ok_or(SetupError::Invalid("family_name"))?;
    s.timezone
        .parse::<Tz>()
        .map_err(|_| SetupError::Invalid("timezone"))?;
    if !auth::valid_pin(&s.pin) {
        return Err(SetupError::Invalid("pin"));
    }
    let child_name = clean_name(&s.child_name).ok_or(SetupError::Invalid("child_name"))?;
    if !valid_avatar(&s.child_avatar) {
        return Err(SetupError::Invalid("child_avatar"));
    }
    let pin_hash = auth::hash_pin(&s.pin);

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if is_set_up(&tx)? {
        return Err(SetupError::AlreadyDone);
    }
    tx.execute(
        "INSERT INTO family (id, name, timezone, pin_hash, default_system_id)
         VALUES (1, ?1, ?2, ?3, 1)",
        params![family_name, s.timezone, pin_hash],
    )?;
    tx.execute(
        "INSERT INTO child (name, avatar) VALUES (?1, ?2)",
        params![child_name, s.child_avatar],
    )?;
    let child_id = tx.last_insert_rowid();
    if s.sample_deck {
        tx.execute("INSERT INTO subject (name) VALUES ('Frans')", [])?;
        let subject_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO deck (subject_id, name, system_id, direction_mode)
             VALUES (?1, 'Voorbeeld: Frans - Nederlands', 1, 'front_to_back')",
            [subject_id],
        )?;
        let deck_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO deck_child (deck_id, child_id) VALUES (?1, ?2)",
            [deck_id, child_id],
        )?;
        for (front, back) in SAMPLE_CARDS {
            tx.execute(
                "INSERT INTO card (deck_id, front_text, back_text) VALUES (?1, ?2, ?3)",
                params![deck_id, front, back],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}
```

Validation happens before the transaction, and the PIN is hashed before it too, so the write lock is held only for the inserts. The "already set up" check is inside the `Immediate` transaction, so two simultaneous setups cannot both pass it.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test family:: && cargo clippy --all-targets -- -D warnings`
Expected: 7 tests pass; clippy clean.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/family.rs
git commit -m "feat: first-run family setup with sample deck"
```

---

### Task 3: HTTP API and parent guard

**Files:**
- Create: `src/server.rs`, `tests/api.rs`
- Modify: `Cargo.toml`, `src/lib.rs`

**Interfaces:**
- Consumes: `auth::{Auth, Login, verify_pin}` (Task 1); `family::{create, is_set_up, pin_hash, children, Setup, SetupError}` (Task 2)
- Produces: `server::AppState { db: Mutex<Connection>, auth: Mutex<Auth>, lan_url: String }`, `server::AppState::new(Connection, String) -> Shared`, `server::Shared = Arc<AppState>`, `server::router(Shared) -> axum::Router`. Error bodies are always `{ "error": "<code>" }`, plus `field` or `retry_after` where the API table says so.

- [ ] **Step 1: Add dependencies and module**

In `Cargo.toml`, add to `[dependencies]`:
```toml
axum = "0.8"
qrcode = { version = "0.14", default-features = false, features = ["svg"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "net"] }
```
and add a section after `[dependencies]`:
```toml
[dev-dependencies]
http-body-util = "0.1"
tower = { version = "0.5", features = ["util"] }
```
In `src/lib.rs`, add after `pub mod family;`:
```rust
pub mod server;
```

- [ ] **Step 2: Write the failing tests**

Create `tests/api.rs`:
```rust
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use wistjet::{db, server};

struct Reply {
    status: StatusCode,
    cookie: Option<String>,
    content_type: String,
    body: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

fn app() -> Router {
    let conn = db::open_in_memory().unwrap();
    server::router(server::AppState::new(
        conn,
        "http://192.168.1.23:8080".into(),
    ))
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Reply {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let header_str = |name| {
        res.headers()
            .get(name)
            .map(|v: &header::HeaderValue| v.to_str().unwrap().to_string())
    };
    Reply {
        status: res.status(),
        cookie: header_str(header::SET_COOKIE),
        content_type: header_str(header::CONTENT_TYPE).unwrap_or_default(),
        body: res.into_body().collect().await.unwrap().to_bytes().to_vec(),
    }
}

fn setup_body() -> Value {
    json!({
        "family_name": "De Smet",
        "timezone": "Europe/Brussels",
        "pin": "2468",
        "child_name": "Lotte",
        "child_avatar": "#e57373",
        "sample_deck": true
    })
}

async fn set_up(app: &Router) {
    let r = call(app, "POST", "/api/setup", None, Some(setup_body())).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
}

/// Logs in and returns the `name=value` cookie pair to send back.
async fn log_in(app: &Router) -> String {
    let r = call(
        app,
        "POST",
        "/api/parent/login",
        None,
        Some(json!({ "pin": "2468" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let cookie = r.cookie.unwrap();
    assert!(cookie.contains("HttpOnly"));
    cookie.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn status_reports_setup_and_lan_url() {
    let app = app();
    let r = call(&app, "GET", "/api/status", None, None).await;
    assert_eq!(
        r.json(),
        json!({ "setup_done": false, "lan_url": "http://192.168.1.23:8080" })
    );
    set_up(&app).await;
    let r = call(&app, "GET", "/api/status", None, None).await;
    assert_eq!(r.json()["setup_done"], json!(true));
}

#[tokio::test]
async fn setup_lists_the_first_child() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "GET", "/api/children", None, None).await;
    assert_eq!(
        r.json(),
        json!([{ "id": 1, "name": "Lotte", "avatar": "#e57373" }])
    );
}

#[tokio::test]
async fn setup_twice_is_a_conflict() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "POST", "/api/setup", None, Some(setup_body())).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn invalid_setup_names_the_field() {
    let app = app();
    let mut body = setup_body();
    body["pin"] = json!("12");
    let r = call(&app, "POST", "/api/setup", None, Some(body)).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json(), json!({ "error": "invalid", "field": "pin" }));
}

#[tokio::test]
async fn parent_routes_need_a_session() {
    let app = app();
    set_up(&app).await;
    for cookie in [None, Some("wistjet_parent=forged"), Some("other=1")] {
        let r = call(&app, "GET", "/api/parent/session", cookie, None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{cookie:?}");
    }
}

#[tokio::test]
async fn right_pin_opens_and_logout_closes_the_parent_area() {
    let app = app();
    set_up(&app).await;
    let cookie = log_in(&app).await;
    let r = call(&app, "GET", "/api/parent/session", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "POST", "/api/parent/logout", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert!(r.cookie.unwrap().contains("Max-Age=0"));
    let r = call(&app, "GET", "/api/parent/session", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_pin_is_rejected_then_locked_out() {
    let app = app();
    set_up(&app).await;
    let wrong = || Some(json!({ "pin": "0000" }));
    for _ in 0..4 {
        let r = call(&app, "POST", "/api/parent/login", None, wrong()).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.json(), json!({ "error": "wrong_pin" }));
    }
    let r = call(&app, "POST", "/api/parent/login", None, wrong()).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.json(), json!({ "error": "locked", "retry_after": 60 }));
    let right = Some(json!({ "pin": "2468" }));
    let r = call(&app, "POST", "/api/parent/login", None, right).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn login_before_setup_is_a_conflict() {
    let app = app();
    let r = call(
        &app,
        "POST",
        "/api/parent/login",
        None,
        Some(json!({ "pin": "2468" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn lan_qr_is_an_svg() {
    let app = app();
    let r = call(&app, "GET", "/api/lan-qr.svg", None, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.content_type, "image/svg+xml");
    assert!(String::from_utf8(r.body).unwrap().contains("<svg"));
}

#[tokio::test]
async fn unknown_api_path_is_not_found() {
    let app = app();
    let r = call(&app, "GET", "/api/nope", None, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --test api`
Expected: compile error, `file not found for module server` (or `cannot find function router`).

- [ ] **Step 4: Implement**

Create `src/server.rs`:
```rust
//! HTTP API and embedded web app. Every `/api/parent/*` route except login and logout
//! requires a parent session (spec §8).

use crate::auth::{self, Auth, Login};
use crate::family::{self, Setup, SetupError};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const COOKIE: &str = "wistjet_parent";

// ponytail: one connection behind a mutex; one family's traffic is tiny.
pub struct AppState {
    pub db: Mutex<Connection>,
    pub auth: Mutex<Auth>,
    /// Address tablets on the home network can use, shown during first run.
    pub lan_url: String,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn new(db: Connection, lan_url: String) -> Shared {
        Arc::new(AppState {
            db: Mutex::new(db),
            auth: Mutex::new(Auth::default()),
            lan_url,
        })
    }
}

pub fn router(state: Shared) -> Router {
    let parent = Router::new()
        .route("/session", get(|| async { StatusCode::NO_CONTENT }))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_parent,
        ));
    Router::new()
        .route("/api/status", get(status))
        .route("/api/setup", post(setup))
        .route("/api/children", get(children))
        .route("/api/lan-qr.svg", get(lan_qr))
        .route("/api/parent/login", post(login))
        .route("/api/parent/logout", post(logout))
        .nest("/api/parent", parent)
        .with_state(state)
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn internal(e: impl std::fmt::Display) -> Response {
    eprintln!("internal error: {e}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|c| c.trim().strip_prefix("wistjet_parent="))
}

async fn require_parent(State(s): State<Shared>, req: Request, next: Next) -> Response {
    let ok = session_token(req.headers())
        .is_some_and(|t| s.auth.lock().unwrap().touch(t, Instant::now()));
    if ok {
        next.run(req).await
    } else {
        error(StatusCode::UNAUTHORIZED, "locked")
    }
}

async fn status(State(s): State<Shared>) -> Response {
    match family::is_set_up(&s.db.lock().unwrap()) {
        Ok(done) => Json(json!({ "setup_done": done, "lan_url": s.lan_url })).into_response(),
        Err(e) => internal(e),
    }
}

async fn setup(State(s): State<Shared>, Json(body): Json<Setup>) -> Response {
    match family::create(&mut s.db.lock().unwrap(), &body) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(SetupError::AlreadyDone) => error(StatusCode::CONFLICT, "already_set_up"),
        Err(SetupError::Invalid(field)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid", "field": field })),
        )
            .into_response(),
        Err(SetupError::Db(e)) => internal(e),
    }
}

async fn children(State(s): State<Shared>) -> Response {
    match family::children(&s.db.lock().unwrap()) {
        Ok(list) => Json(list).into_response(),
        Err(e) => internal(e),
    }
}

async fn lan_qr(State(s): State<Shared>) -> Response {
    match qrcode::QrCode::new(&s.lan_url) {
        Ok(code) => {
            let svg = code
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(200, 200)
                .build();
            ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response()
        }
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct PinBody {
    pin: String,
}

async fn login(State(s): State<Shared>, Json(body): Json<PinBody>) -> Response {
    let hash = family::pin_hash(&s.db.lock().unwrap());
    let hash = match hash {
        Ok(Some(h)) => h,
        Ok(None) => return error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => return internal(e),
    };
    let result = s
        .auth
        .lock()
        .unwrap()
        .login(Instant::now(), || auth::verify_pin(&body.pin, &hash));
    match result {
        Login::Ok(token) => (
            StatusCode::NO_CONTENT,
            [(
                header::SET_COOKIE,
                format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/"),
            )],
        )
            .into_response(),
        Login::WrongPin => error(StatusCode::UNAUTHORIZED, "wrong_pin"),
        Login::Locked { retry_after } => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "locked", "retry_after": retry_after.as_secs().max(1) })),
        )
            .into_response(),
    }
}

async fn logout(State(s): State<Shared>, headers: HeaderMap) -> Response {
    if let Some(t) = session_token(&headers) {
        s.auth.lock().unwrap().logout(t);
    }
    (
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
        )],
    )
        .into_response()
}
```

Notes for the implementer:
- `login` and `logout` sit outside the guarded router: logging in obviously needs no session, and logging out must work even after the session expired.
- `/api/parent/session` exists so the web app can ask "am I unlocked?" on load. It is also the guarded route the tests probe.
- Without a fallback, unknown paths get `axum`'s default empty 404. Task 4 adds the static-file fallback, which keeps `/api/*` as a JSON 404.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass (10 in `api`); fmt and clippy clean.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/server.rs tests/api.rs
git commit -m "feat: HTTP API with server-enforced parent PIN"
```

---

### Task 4: Web app, embedded

**Files:**
- Create: `web/package.json`, `web/package-lock.json` (generated), `web/tsconfig.json`, `web/vite.config.ts`, `web/index.html`, `web/src/main.ts`, `web/src/lib/api.ts`, `web/src/App.svelte`, `web/src/routes/Profiles.svelte`, `web/src/routes/Parent.svelte`, `web/src/routes/Setup.svelte`, `web/src/app.css`
- Modify: `Cargo.toml`, `src/server.rs`, `tests/api.rs`, `.gitignore`, `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the API table above (Task 3)
- Produces: `web/dist/` (build output, git-ignored, embedded into the binary); hash routes `#/` (profile select) and `#/ouder` (parent area), which Plans 3 and 4 extend

- [ ] **Step 1: Write the failing test**

Append to `tests/api.rs`:
```rust
/// Needs the built web app: run `npm --prefix web run build` first.
#[tokio::test]
async fn web_app_is_served_for_non_api_paths() {
    let app = app();
    for path in ["/", "/index.html", "/some/client/route"] {
        let r = call(&app, "GET", path, None, None).await;
        assert_eq!(r.status, StatusCode::OK, "{path}");
        assert!(r.content_type.starts_with("text/html"), "{path}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test api web_app`
Expected: FAIL, status 404 for `/` (no fallback yet).

- [ ] **Step 3: Create the web app**

`web/package.json`:
```json
{
  "name": "wistjet-web",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "check": "svelte-check"
  },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^7.3.0",
    "@tsconfig/svelte": "^5.0.8",
    "svelte": "^5.57.0",
    "svelte-check": "^4.7.6",
    "typescript": "~6.0.2",
    "vite": "^8.3.0"
  }
}
```

`web/tsconfig.json`:
```json
{
  "extends": "@tsconfig/svelte/tsconfig.json",
  "compilerOptions": {
    "target": "es2023",
    "module": "esnext",
    "types": ["svelte", "vite/client"],
    "strict": true,
    "noEmit": true
  },
  "include": ["src/**/*.ts", "src/**/*.svelte"]
}
```

`web/vite.config.ts`:
```ts
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// `npm run dev` serves the UI with hot reload and forwards API calls to `cargo run`.
export default defineConfig({
  plugins: [svelte()],
  server: { proxy: { '/api': 'http://localhost:8080' } },
})
```

`web/index.html`:
```html
<!doctype html>
<html lang="nl">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🃏</text></svg>" />
    <title>Wistjet</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

`web/src/main.ts`:
```ts
import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

mount(App, { target: document.getElementById('app')! })
```

`web/src/lib/api.ts`:
```ts
/** Error body the server sends: `{ error, field?, retry_after? }`. */
export type ErrorBody = { error?: string; field?: string; retry_after?: number } | null

export class ApiError extends Error {
  constructor(
    public status: number,
    public body: ErrorBody,
  ) {
    super(`HTTP ${status}`)
  }
}

/** GET `/api/<path>`, or POST it as JSON when `body` is given. */
export async function api<T = unknown>(path: string, body?: unknown): Promise<T> {
  const init: RequestInit =
    body === undefined
      ? {}
      : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }
  const res = await fetch(`/api/${path}`, init)
  const data = res.headers.get('content-type')?.includes('json') ? await res.json() : null
  if (!res.ok) throw new ApiError(res.status, data)
  return data as T
}
```

`web/src/App.svelte` routes by URL hash and shows setup until the family exists:
```svelte
<script lang="ts">
  import { api } from './lib/api'
  import Parent from './routes/Parent.svelte'
  import Profiles from './routes/Profiles.svelte'
  import Setup from './routes/Setup.svelte'

  type Status = { setup_done: boolean; lan_url: string }

  let status = $state<Status | null>(null)
  let failed = $state(false)
  let route = $state(location.hash)

  $effect(() => {
    const onHash = () => (route = location.hash)
    addEventListener('hashchange', onHash)
    return () => removeEventListener('hashchange', onHash)
  })

  async function load() {
    try {
      status = await api<Status>('status')
      failed = false
    } catch {
      failed = true
    }
  }
  load()
</script>

<main>
  {#if failed}
    <div class="card narrow">
      <p>Wistjet is niet bereikbaar. Draait het programma nog?</p>
      <div class="actions"><button onclick={load}>Opnieuw proberen</button></div>
    </div>
  {:else if !status}
    <p class="notice">Laden…</p>
  {:else if !status.setup_done}
    <Setup lanUrl={status.lan_url} onDone={load} />
  {:else if route === '#/ouder'}
    <Parent />
  {:else}
    <Profiles />
  {/if}
</main>
```

`web/src/routes/Profiles.svelte` (tiles become links in Plan 3):
```svelte
<script lang="ts">
  import { api } from '../lib/api'

  type Child = { id: number; name: string; avatar: string }

  const children = api<Child[]>('children')
</script>

<header class="bar">
  <h1>Wie gaat er oefenen?</h1>
  <a class="icon-button" href="#/ouder" aria-label="Ouderzone" title="Ouderzone">🔒</a>
</header>

{#await children}
  <p class="notice">Laden…</p>
{:then list}
  <ul class="tiles">
    {#each list as child (child.id)}
      <li class="tile">
        <span class="avatar" style:background={child.avatar}>{child.name[0]}</span>
        <span class="tile-name">{child.name}</span>
      </li>
    {/each}
  </ul>
{:catch}
  <p class="notice">De kinderen konden niet geladen worden.</p>
{/await}
```

`web/src/routes/Parent.svelte` (PIN screen, then a placeholder Plan 4 replaces; locks after 10 idle minutes):
```svelte
<script lang="ts">
  import { api, ApiError } from '../lib/api'

  const IDLE_MS = 10 * 60 * 1000

  let unlocked = $state<boolean | null>(null)
  let pin = $state('')
  let message = $state('')

  api('parent/session').then(
    () => (unlocked = true),
    () => (unlocked = false),
  )

  async function unlock(e: SubmitEvent) {
    e.preventDefault()
    message = ''
    try {
      await api('parent/login', { pin })
      unlocked = true
    } catch (err) {
      if (err instanceof ApiError && err.status === 429) {
        message = `Te veel pogingen. Probeer opnieuw over ${err.body?.retry_after ?? 60} seconden.`
      } else if (err instanceof ApiError && err.status === 401) {
        message = 'Die pincode klopt niet.'
      } else {
        message = 'Er ging iets mis. Probeer het opnieuw.'
      }
    }
    pin = ''
  }

  async function lock() {
    await api('parent/logout', {}).catch(() => {})
    location.hash = '#/'
  }

  // Lock after 10 minutes without activity; the server enforces the same limit.
  $effect(() => {
    if (!unlocked) return
    let timer = setTimeout(lock, IDLE_MS)
    const reset = () => {
      clearTimeout(timer)
      timer = setTimeout(lock, IDLE_MS)
    }
    const events = ['pointerdown', 'keydown'] as const
    events.forEach((e) => addEventListener(e, reset))
    return () => {
      clearTimeout(timer)
      events.forEach((e) => removeEventListener(e, reset))
    }
  })
</script>

{#if unlocked === null}
  <p class="notice">Laden…</p>
{:else if !unlocked}
  <form class="card narrow" onsubmit={unlock}>
    <h1>Ouderzone</h1>
    <label>
      Pincode
      <input type="password" inputmode="numeric" autocomplete="current-password" bind:value={pin} required />
    </label>
    {#if message}<p class="error" role="alert">{message}</p>{/if}
    <div class="actions">
      <a class="button secondary" href="#/">Terug</a>
      <button type="submit">Ontgrendel</button>
    </div>
  </form>
{:else}
  <header class="bar">
    <h1>Ouderzone</h1>
    <button class="secondary" onclick={lock}>Vergrendel</button>
  </header>
  <p class="card">Hier beheer je binnenkort kinderen, vakken en decks.</p>
{/if}
```

`web/src/routes/Setup.svelte` (spec §7.8; the sample deck is offered on step 3, and step 4 shows the QR code for tablets):
```svelte
<script lang="ts">
  import { api, ApiError } from '../lib/api'

  let { lanUrl, onDone }: { lanUrl: string; onDone: () => void } = $props()

  const COLOURS = ['#e57373', '#f06292', '#ba68c8', '#7986cb', '#4fc3f7', '#4db6ac', '#aed581', '#ffb74d']
  const FIELD_STEP: Record<string, number> = { family_name: 1, timezone: 1, pin: 1, child_name: 2, child_avatar: 2 }
  const FIELD_MESSAGE: Record<string, string> = {
    family_name: 'Geef een familienaam van hoogstens 50 tekens.',
    timezone: 'Kies een tijdzone uit de lijst.',
    pin: 'De pincode bestaat uit 4 tot 8 cijfers.',
    child_name: 'Geef een naam van hoogstens 50 tekens.',
    child_avatar: 'Kies een kleur.',
  }

  const browserZone = Intl.DateTimeFormat().resolvedOptions().timeZone
  const zones = Intl.supportedValuesOf('timeZone')
  if (!zones.includes(browserZone)) zones.unshift(browserZone)

  let step = $state(1)
  let familyName = $state('')
  let timezone = $state(browserZone)
  let pin = $state('')
  let pinAgain = $state('')
  let childName = $state('')
  let childAvatar = $state(COLOURS[0])
  let sampleDeck = $state(true)
  let error = $state('')
  let busy = $state(false)

  function next(e: SubmitEvent) {
    e.preventDefault()
    error = ''
    if (step === 1 && !/^\d{4,8}$/.test(pin)) {
      error = FIELD_MESSAGE.pin
      return
    }
    if (step === 1 && pin !== pinAgain) {
      error = 'De twee pincodes zijn niet gelijk.'
      return
    }
    step += 1
  }

  function back() {
    error = ''
    step -= 1
  }

  async function finish(e: SubmitEvent) {
    e.preventDefault()
    error = ''
    busy = true
    try {
      await api('setup', {
        family_name: familyName,
        timezone,
        pin,
        child_name: childName,
        child_avatar: childAvatar,
        sample_deck: sampleDeck,
      })
      step = 4
    } catch (err) {
      const field = err instanceof ApiError ? err.body?.field : undefined
      if (err instanceof ApiError && err.status === 409) {
        onDone() // set up meanwhile, e.g. in another tab
      } else if (field && field in FIELD_STEP) {
        step = FIELD_STEP[field]
        error = FIELD_MESSAGE[field]
      } else {
        error = 'Opslaan lukte niet. Probeer het opnieuw.'
      }
    } finally {
      busy = false
    }
  }
</script>

<div class="card narrow">
  {#if step < 4}<p class="steps">Stap {step} van 3</p>{/if}

  {#if step === 1}
    <form onsubmit={next}>
      <h1>Welkom bij Wistjet</h1>
      <label>Familienaam <input bind:value={familyName} maxlength="50" required /></label>
      <label>
        Tijdzone
        <select bind:value={timezone}>
          {#each zones as zone (zone)}<option>{zone}</option>{/each}
        </select>
      </label>
      <label>
        Pincode voor de ouderzone (4 tot 8 cijfers)
        <input type="password" inputmode="numeric" autocomplete="new-password" bind:value={pin} required />
      </label>
      <label>
        Herhaal de pincode
        <input type="password" inputmode="numeric" autocomplete="new-password" bind:value={pinAgain} required />
      </label>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions"><button type="submit">Volgende</button></div>
    </form>
  {:else if step === 2}
    <form onsubmit={next}>
      <h1>Wie gaat er oefenen?</h1>
      <label>Naam van je kind <input bind:value={childName} maxlength="50" required /></label>
      <fieldset class="swatches">
        <legend>Kleur</legend>
        {#each COLOURS as colour, i (colour)}
          <label class="swatch" style:background={colour}>
            <input type="radio" name="colour" value={colour} bind:group={childAvatar} />
            <span class="sr-only">Kleur {i + 1}</span>
          </label>
        {/each}
      </fieldset>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button type="button" class="secondary" onclick={back}>Terug</button>
        <button type="submit">Volgende</button>
      </div>
    </form>
  {:else if step === 3}
    <form onsubmit={finish}>
      <h1>Meteen proberen?</h1>
      <label class="choice">
        <input type="radio" name="sample" value={true} bind:group={sampleDeck} />
        Ja, voeg een klein voorbeelddeck Frans – Nederlands toe
      </label>
      <label class="choice">
        <input type="radio" name="sample" value={false} bind:group={sampleDeck} />
        Nee, ik begin met een lege Wistjet
      </label>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button type="button" class="secondary" onclick={back}>Terug</button>
        <button type="submit" disabled={busy}>Klaar</button>
      </div>
    </form>
  {:else}
    <h1>Klaar!</h1>
    <p>Open Wistjet op een tablet in hetzelfde wifi-netwerk door deze code te scannen:</p>
    <img class="qr" src="/api/lan-qr.svg" alt="QR-code naar {lanUrl}" />
    <p class="url">{lanUrl}</p>
    <div class="actions"><button onclick={onDone}>Start</button></div>
  {/if}
</div>
```

`web/src/app.css`:
```css
:root {
  --bg: #fbf7f0;
  --surface: #ffffff;
  --ink: #2b2a33;
  --muted: #6b6878;
  --line: #e7e1d6;
  --accent: #e8643c;
  --accent-ink: #ffffff;
  --error: #c0392b;
  --radius: 18px;
  font-family: ui-rounded, 'SF Pro Rounded', system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
  color: var(--ink);
  background: var(--bg);
  -webkit-font-smoothing: antialiased;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  background: var(--bg);
}

main {
  max-width: 960px;
  margin: 0 auto;
  padding: 24px 16px 48px;
}

h1 {
  font-size: 1.75rem;
  margin: 0 0 16px;
}

.notice {
  color: var(--muted);
  text-align: center;
  margin-top: 20vh;
}

.card {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  padding: 24px;
  box-shadow: 0 2px 12px rgb(43 42 51 / 6%);
}

.narrow {
  max-width: 440px;
  margin: 8vh auto 0;
}

.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 24px;
}

.bar h1 {
  margin: 0;
}

label {
  display: block;
  margin-bottom: 16px;
  font-weight: 600;
}

input:not([type='radio']),
select {
  display: block;
  width: 100%;
  margin-top: 6px;
  padding: 12px 14px;
  font: inherit;
  font-weight: 400;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--surface);
  color: var(--ink);
}

input:focus-visible,
select:focus-visible,
button:focus-visible,
a:focus-visible {
  outline: 3px solid color-mix(in srgb, var(--accent) 50%, transparent);
  outline-offset: 2px;
}

button,
.button {
  display: inline-block;
  padding: 12px 22px;
  font: inherit;
  font-weight: 700;
  border: none;
  border-radius: 999px;
  background: var(--accent);
  color: var(--accent-ink);
  cursor: pointer;
  text-decoration: none;
  min-height: 48px;
}

button:disabled {
  opacity: 0.6;
  cursor: default;
}

.secondary {
  background: transparent;
  color: var(--ink);
  border: 1px solid var(--line);
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  margin-top: 8px;
}

.error {
  color: var(--error);
  font-weight: 600;
}

.steps {
  color: var(--muted);
  margin: 0 0 4px;
  font-size: 0.9rem;
}

.choice {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  font-weight: 400;
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: 12px;
  cursor: pointer;
}

.swatches {
  border: none;
  padding: 0;
  margin: 0 0 16px;
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.swatches legend {
  font-weight: 600;
  margin-bottom: 8px;
}

.swatch {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  margin: 0;
  cursor: pointer;
  border: 3px solid transparent;
}

.swatch:has(input:checked) {
  border-color: var(--ink);
}

.swatch:has(input:focus-visible) {
  outline: 3px solid color-mix(in srgb, var(--accent) 50%, transparent);
  outline-offset: 2px;
}

.swatch input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.qr {
  display: block;
  width: 200px;
  height: 200px;
  margin: 8px auto;
}

.url {
  text-align: center;
  font-family: ui-monospace, Menlo, Consolas, monospace;
  overflow-wrap: anywhere;
}

.icon-button {
  font-size: 1.4rem;
  text-decoration: none;
  padding: 8px;
  border-radius: 12px;
}

.tiles {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 16px;
}

.tile {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 24px 12px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius);
}

.avatar {
  width: 88px;
  height: 88px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  font-size: 2.4rem;
  font-weight: 800;
  color: #ffffff;
}

.tile-name {
  font-size: 1.25rem;
  font-weight: 700;
}
```

Then install, which creates `web/package-lock.json`:

Run: `npm --prefix web install`
Expected: about 47 packages added, no errors.

- [ ] **Step 4: Check and build the web app**

Run: `npm --prefix web run check && npm --prefix web run build`
Expected: `svelte-check` reports 0 errors and 0 warnings; Vite writes `web/dist/index.html` and `web/dist/assets/*`.

- [ ] **Step 5: Embed and serve it**

In `Cargo.toml`, add to `[dependencies]`:
```toml
rust-embed = { version = "8", features = ["mime-guess"] }
```
In `src/server.rs`, change the `axum::http` import to:
```rust
use axum::http::{HeaderMap, StatusCode, Uri, header};
```
Add before `pub fn router`:
```rust
#[derive(rust_embed::Embed)]
#[folder = "web/dist/"]
#[allow_missing = true]
struct Assets;
```
In `router`, add `.fallback(static_file)` between `.nest("/api/parent", parent)` and `.with_state(state)`.
Append to `src/server.rs`:
```rust
/// Serves the embedded web app; unknown non-API paths get `index.html`.
async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return error(StatusCode::NOT_FOUND, "not_found");
    }
    match Assets::get(path).or_else(|| Assets::get("index.html")) {
        Some(file) => (
            [(header::CONTENT_TYPE, file.metadata.mimetype().to_string())],
            file.data,
        )
            .into_response(),
        None => error(StatusCode::NOT_FOUND, "not_found"),
    }
}
```

In debug builds `rust-embed` reads `web/dist` from disk at run time; in release builds the files are compiled into the binary. `allow_missing` lets the crate compile before the first web build.

`.gitignore` becomes:
```
/target
/web/node_modules
/web/dist
```

`.github/workflows/ci.yml` becomes:
```yaml
name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: web/package-lock.json
      - run: npm --prefix web ci
      - run: npm --prefix web run check
      - run: npm --prefix web run build
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass (11 in `api`); fmt and clippy clean.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/server.rs tests/api.rs .gitignore .github web
git commit -m "feat: embedded Svelte app with first-run wizard and parent PIN screen"
```

---

### Task 5: Launcher

**Files:**
- Create: `src/main.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `db::open` (Plan 1), `server::{AppState::new, router}` (Task 3)
- Produces: the `wistjet` binary. Environment: `WISTJET_DATA_DIR` (default: OS per-user data folder, e.g. `~/Library/Application Support/Wistjet`, `%APPDATA%\Wistjet\data`), `WISTJET_PORT` (default 8080), `WISTJET_OPEN_BROWSER` (`0` disables; Plan 5's Docker image sets it). Database file: `<data dir>/wistjet.db`.

- [ ] **Step 1: Add dependencies**

In `Cargo.toml`, add to `[dependencies]`:
```toml
directories = "6"
local-ip-address = "0.6"
open = "5"
```

- [ ] **Step 2: Write the failing tests**

Create `src/main.rs` with the doc comment and the test module:
```rust
//! Launcher: opens the data folder, starts the server on the home network and opens the browser.

#[cfg(test)]
mod tests {
    use super::*;

    fn config(vars: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_env(|k| {
            vars.iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| v.to_string())
        })
    }

    #[test]
    fn defaults() {
        assert_eq!(
            config(&[]),
            Ok(Config {
                data_dir: None,
                port: 8080,
                open_browser: true
            })
        );
    }

    #[test]
    fn overrides() {
        assert_eq!(
            config(&[
                ("WISTJET_DATA_DIR", "/data"),
                ("WISTJET_PORT", "9000"),
                ("WISTJET_OPEN_BROWSER", "0"),
            ]),
            Ok(Config {
                data_dir: Some("/data".into()),
                port: 9000,
                open_browser: false
            })
        );
    }

    #[test]
    fn bad_port_is_an_error() {
        assert!(config(&[("WISTJET_PORT", "tachtig")]).is_err());
        assert!(config(&[("WISTJET_PORT", "70000")]).is_err());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --bin wistjet`
Expected: compile errors, `cannot find type Config`, and `main function not found`.

- [ ] **Step 4: Implement**

Insert between the doc comment and `#[cfg(test)]` in `src/main.rs`:
```rust
use std::path::PathBuf;
use wistjet::{db, server};

#[derive(Debug, PartialEq)]
struct Config {
    /// `None` = the operating system's per-user data folder.
    data_dir: Option<PathBuf>,
    port: u16,
    open_browser: bool,
}

impl Config {
    fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let port = match get("WISTJET_PORT") {
            Some(p) => p
                .parse()
                .map_err(|_| format!("WISTJET_PORT is geen geldige poort: {p}"))?,
            None => 8080,
        };
        Ok(Config {
            data_dir: get("WISTJET_DATA_DIR").map(PathBuf::from),
            port,
            open_browser: get("WISTJET_OPEN_BROWSER").is_none_or(|v| v != "0"),
        })
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Wistjet kon niet starten: {e}");
        // A double-clicked window would otherwise close before the message can be read.
        eprintln!("Druk op Enter om af te sluiten.");
        let _ = std::io::stdin().read_line(&mut String::new());
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env(|k| std::env::var(k).ok())?;
    let data_dir = match config.data_dir {
        Some(d) => d,
        None => directories::ProjectDirs::from("", "", "Wistjet")
            .ok_or("geen gegevensmap gevonden")?
            .data_dir()
            .to_path_buf(),
    };
    let local = format!("http://localhost:{}", config.port);

    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            // Most likely Wistjet is already running: show it instead of failing.
            println!("Wistjet draait al op {local}.");
            if config.open_browser {
                let _ = open::that(&local);
            }
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    std::fs::create_dir_all(&data_dir)?;
    let conn = db::open(&data_dir.join("wistjet.db")).map_err(|e| format!("{e:?}"))?;
    let lan_url = local_ip_address::local_ip()
        .map(|ip| format!("http://{ip}:{}", config.port))
        .unwrap_or_else(|_| local.clone());

    println!("Wistjet draait.");
    println!("  Op deze computer:  {local}");
    println!("  Op een tablet:     {lan_url}");
    println!("  Gegevens:          {}", data_dir.display());
    println!("Sluit dit venster om Wistjet te stoppen.");
    if config.open_browser {
        let _ = open::that(&local);
    }
    axum::serve(
        listener,
        server::router(server::AppState::new(conn, lan_url)),
    )
    .await?;
    Ok(())
}
```

Why the port is bound before anything else: if it is already taken, Wistjet is almost certainly running already (someone double-clicked twice), so the launcher opens the browser on it and exits instead of showing an error.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass (3 in the `wistjet` binary); fmt and clippy clean.

- [ ] **Step 6: Smoke-test the release binary**

Run:
```bash
npm --prefix web run build && cargo build --release
D=$(mktemp -d); B=http://localhost:18080
WISTJET_DATA_DIR=$D WISTJET_PORT=18080 WISTJET_OPEN_BROWSER=0 ./target/release/wistjet > $D/out.log 2>&1 &
sleep 1; cat $D/out.log
curl -s $B/api/status; echo
curl -s $B/ | head -c 60; echo
curl -s -o /dev/null -w "setup %{http_code}\n" -H 'Content-Type: application/json' \
  -d '{"family_name":"De Smet","timezone":"Europe/Brussels","pin":"2468","child_name":"Lotte","child_avatar":"#e57373","sample_deck":true}' $B/api/setup
curl -s -o /dev/null -w "guarded without login %{http_code}\n" $B/api/parent/session
curl -s -c $D/c -o /dev/null -w "login %{http_code}\n" -H 'Content-Type: application/json' -d '{"pin":"2468"}' $B/api/parent/login
curl -s -b $D/c -o /dev/null -w "guarded with login %{http_code}\n" $B/api/parent/session
WISTJET_DATA_DIR=$D WISTJET_PORT=18080 WISTJET_OPEN_BROWSER=0 ./target/release/wistjet
kill %1; ls $D
```
Expected:
- the banner lists `http://localhost:18080`, a LAN address and the data folder
- status `{"lan_url":"http://<lan-ip>:18080","setup_done":false}`
- the page starts with `<!doctype html>`
- `setup 204`, `guarded without login 401`, `login 204`, `guarded with login 204`
- the second start prints `Wistjet draait al op http://localhost:18080.` and exits
- the data folder holds `wistjet.db`

The release binary is about 4 MB.

- [ ] **Step 7: Look at it in a browser**

Run `cargo run --release` (opens the browser), finish the wizard with the sample deck, check the QR code shows, click Start, see the child tile, open the lock icon, try a wrong PIN, then the right one, then "Vergrendel". Delete the data folder afterwards if it was your real one.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: launcher that serves on the home network and opens the browser"
```

---

## Done when

- `cargo test` passes (Plan 1's 43 tests plus 7 auth, 7 family, 11 api and 3 launcher tests = 71), and the web check and build are clean.
- The smoke script in Task 5 shows the expected output, and the wizard works in a browser.
- Spec coverage after this plan: §6.1 profile select (without due badges, Plan 3), §7 PIN entry and auto-lock, §7.8 first run, §8 PIN enforcement, hashing, lockout, session expiry, input limits for names, §9.1 one data location and automatic migrations at start. The rest of §11's "PIN enforcement" test is `parent_routes_need_a_session`.
