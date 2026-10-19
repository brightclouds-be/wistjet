# Plan 5a: Backup, Restore & End-to-End Test — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A parent can download a backup of everything and restore one, even from an older Wistjet. Wistjet also writes a daily backup to a configurable folder and keeps the last 14. The spec's end-to-end test runs in CI against the real app in Chrome.

**Architecture:** A backup is the SQLite database itself, written with `VACUUM INTO`, so it is one consistent file with images, progress, review log and settings.
- **Restore** writes the upload to a temporary file and opens it with `db::open`. That checks it is a Wistjet database and not from a newer version, and migrates an older one. Only then does SQLite's backup API copy it into the live connection: the copy is atomic and needs no file swapping while the database is open. Every parent session ends, because the restored PIN may differ.
- **Automatic backups:** a background task checks at start and every 10 minutes whether today's file exists, writes it under a temporary name, renames it, and prunes older files.
- **End-to-end test:** `e2e/e2e.mjs` drives Chrome through its DevTools protocol, using only what Node 22 has built in.

**Tech Stack:** as before; `rusqlite` gains its `backup` feature. The end-to-end test needs Node 22 and Chrome; GitHub's Ubuntu runners include Chrome.

**Spec:** `docs/superpowers/specs/2026-10-08-family-flashcards-design.md` (§9.2, §10 "Restore of an invalid or unreadable file…", §11 backup round trip and end-to-end test). Roadmap: `docs/superpowers/plans/2026-10-11-wistjet-roadmap.md`.

**Split:** Plan 5 is split in two. This plan (5a) covers data safety and the end-to-end test. Plan 5b covers distribution: desktop builds with a tray icon, the Docker image, `compose.yaml`, a Kubernetes example and the README.

## Decisions made while planning

- **The backup file is the database**, named `wistjet-YYYY-MM-DD.db` in the family's timezone. Nothing new to learn or version: `db::open` already rejects files that aren't Wistjet's or are from a newer version, and migrates older ones.
- **Restore copies into the live connection** with SQLite's backup API instead of swapping files. The database stays open, no `-wal`/`-shm` files get mixed up, and it also works in tests on in-memory databases.
- **An empty Wistjet database is refused as a backup,** because it would wipe the family. The same goes for any file that isn't a Wistjet backup; the message says nothing was changed.
- **A restore ends every parent session,** since the restored PIN may be another one.
- **Restore uploads must be `application/octet-stream`,** which a plain HTML form can't send. It's the same CSRF guard as image uploads. Uploads may be up to 1 GiB, since backups hold every photo.
- **Automatic backups:**
  - **Folder:** `WISTJET_BACKUP_DIR`, defaulting to `backups` inside the data folder.
  - **Retention:** `WISTJET_BACKUP_KEEP` (default 14).
  - **Timing:** checked at start and every 10 minutes, so a laptop opened for a few minutes still gets its daily backup.
  - **Safety:** files are written under a `.partial` name and renamed, so a half-written file never counts as a backup. Pruning only touches files named like a backup.
- **The download is built in memory** (`ponytail:` note). That's fine for a family's cards and photos; stream it from disk if that ever stops being true.
- **The end-to-end test uses no test framework:** Node's built-in `fetch` and `WebSocket` drive Chrome over the DevTools protocol, the way every click-through in Plans 3–4b was verified. It runs in CI after a release build.

## Global Constraints

- A bad or newer backup must leave the current data untouched, with a clear Dutch message (spec §10).
- Backup and restore routes live under `/api/parent/` and need a parent session.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `npm --prefix web run check` (0 errors, 0 warnings) and `npm --prefix web test` clean; `node e2e/e2e.mjs target/release/wistjet` passes.

## Review Focus

1. **Restoring a photo, a random file, another app's SQLite file, or an empty installation's backup:** refused, and nothing changes. Tests `a_bad_file_changes_nothing` (Task 1), `a_bad_restore_is_refused_and_changes_nothing` (Task 2).
2. **A backup from a newer Wistjet:** refused with the "werk eerst bij" (update first) message. Test `a_backup_from_a_newer_version_is_refused` (Task 1).
3. **The app stopped halfway through writing the daily backup:** the half-written file never counts as a backup, and the next run writes a fresh one. The `.partial` name in `daily` (Task 1).
4. **A NAS share (or anyone) also storing other files in the backup folder:** they are never pruned. Test `only_the_newest_backups_are_kept_and_other_files_are_left_alone` (Task 1).
5. **The first day after setup on a laptop that is only open briefly:** the daily backup still happens. The 10-minute check, plus the backup check in the Task 3 click-through.

## File Structure

| File | Responsibility |
|---|---|
| `src/backup.rs` | Write, download, restore, daily backup with pruning, the background task |
| `src/auth.rs` | + `end_all_sessions` |
| `src/server.rs` | `AppState` gains `backups: Option<BackupConfig>` |
| `src/parent_api.rs` | + `GET /backup`, `GET /backups`, `POST /restore` |
| `src/main.rs` | `WISTJET_BACKUP_DIR`, `WISTJET_BACKUP_KEEP`; starts the backup task; banner line |
| `web/src/lib/api.ts` | `upload` with an explicit type; `download()` |
| `web/src/routes/parent/Settings.svelte` | + "Back-up" section |
| `e2e/e2e.mjs` | The spec §11 end-to-end test |
| `.github/workflows/ci.yml` | + release build and end-to-end test |

API (all need a parent session):

| Method & path | Result |
|---|---|
| `GET /api/parent/backup` | the database file; `Content-Disposition: attachment; filename="wistjet-YYYY-MM-DD.db"` |
| `POST /api/parent/restore` | body = backup file, `Content-Type: application/octet-stream` → 204 and all sessions end; 400 field `backup_file` or `backup_newer`; 415 for other types |
| `GET /api/parent/backups` | `{ dir, keep, latest }` (`dir` null when automatic backups are off, as in tests) |

---

### Task 1: Backup and restore

**Files:**
- Create: `src/backup.rs`
- Modify: `Cargo.toml`, `src/lib.rs`

**Interfaces:**
- Consumes: `db::{open, open_in_memory, OpenError}`, `family::is_set_up`, `server::{Shared, today}`
- Produces: `backup::{DEFAULT_KEEP, BackupConfig { dir, keep }, RestoreError { NotABackup, TooNew, Io, Db }, write_backup, backup_bytes, restore, daily, prune, latest, auto_backups}`

- [ ] **Step 1: Write the failing tests**

In `Cargo.toml`, the `rusqlite` line becomes:
```toml
rusqlite = { version = "0.37", features = ["backup", "bundled", "chrono"] }
```
In `src/lib.rs`, add after `pub mod auth;`:
```rust
pub mod backup;
```
Create `src/backup.rs` with the doc comment and the test module:
```rust
//! Backups (spec §9.2). A backup is the SQLite database itself: content, images, progress,
//! review log and settings in one file. Restoring checks and upgrades the file in a temporary
//! copy first and only then copies it into the live database, so a bad file changes nothing.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::Setup;
    use crate::store::{Answer, record_answer};

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    /// A family with the sample deck, an answer and an image.
    fn family() -> Connection {
        let mut conn = db::open_in_memory().unwrap();
        family::create(
            &mut conn,
            &Setup {
                family_name: "De Smet".into(),
                timezone: "Europe/Brussels".into(),
                pin: "2468".into(),
                child_name: "Lotte".into(),
                child_avatar: "#e57373".into(),
                sample_deck: true,
            },
        )
        .unwrap();
        let a = Answer {
            id: "a1".into(),
            child_id: 1,
            card_id: 1,
            direction: 0,
            known: true,
        };
        record_answer(
            &mut conn,
            &a,
            "2026-10-19T09:00:00Z".parse().unwrap(),
            d("2026-10-19"),
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO image (mime, data) VALUES ('image/png', x'89504E470D0A1A0A')",
        )
        .unwrap();
        conn
    }

    /// Every row of every table, as text: equal dumps mean equal data.
    fn dump(conn: &Connection) -> Vec<String> {
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let mut rows = vec![];
        for t in tables {
            let mut stmt = conn
                .prepare(&format!("SELECT * FROM \"{t}\" ORDER BY 1, 2"))
                .unwrap();
            let n = stmt.column_count();
            let mut q = stmt.query([]).unwrap();
            while let Some(r) = q.next().unwrap() {
                let cols: Vec<String> = (0..n)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect();
                rows.push(format!("{t}: {}", cols.join(" | ")));
            }
        }
        rows
    }

    fn a_dir(name: &str) -> PathBuf {
        let dir = temp_path(name);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn backup_then_restore_gives_identical_data() {
        let mut conn = family();
        let before = dump(&conn);
        let bytes = backup_bytes(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO child (name, avatar) VALUES ('Wout', '#4fc3f7');
             UPDATE deck SET archived = 1; DELETE FROM progress; UPDATE family SET name = 'Anders';",
        )
        .unwrap();
        assert_ne!(dump(&conn), before);
        restore(&mut conn, &bytes).unwrap();
        assert_eq!(dump(&conn), before);
    }

    #[test]
    fn a_backup_restores_into_an_empty_installation() {
        let original = family();
        let bytes = backup_bytes(&original).unwrap();
        let mut fresh = db::open_in_memory().unwrap();
        restore(&mut fresh, &bytes).unwrap();
        assert_eq!(dump(&fresh), dump(&original));
    }

    #[test]
    fn a_bad_file_changes_nothing() {
        let mut conn = family();
        let before = dump(&conn);
        let empty_install = backup_bytes(&db::open_in_memory().unwrap()).unwrap();
        let other_app = {
            let path = temp_path("other");
            Connection::open(&path)
                .unwrap()
                .execute_batch("CREATE TABLE notes (x)")
                .unwrap();
            let bytes = fs::read(&path).unwrap();
            remove_db_files(&path);
            bytes
        };
        for bytes in [
            b"not a database at all".to_vec(),
            vec![],
            empty_install,
            other_app,
        ] {
            assert!(matches!(
                restore(&mut conn, &bytes),
                Err(RestoreError::NotABackup)
            ));
            assert_eq!(dump(&conn), before);
        }
    }

    #[test]
    fn a_backup_from_a_newer_version_is_refused() {
        let mut conn = family();
        let path = temp_path("newer");
        write_backup(&conn, &path).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute_batch("PRAGMA user_version = 99")
            .unwrap();
        let bytes = fs::read(&path).unwrap();
        remove_db_files(&path);
        assert!(matches!(
            restore(&mut conn, &bytes),
            Err(RestoreError::TooNew)
        ));
    }

    #[test]
    fn one_automatic_backup_per_day() {
        let conn = family();
        let dir = a_dir("daily");
        let made = daily(&conn, &dir, d("2026-10-19"), 14).unwrap();
        assert_eq!(made, Some(dir.join("wistjet-2026-10-19.db")));
        assert_eq!(daily(&conn, &dir, d("2026-10-19"), 14).unwrap(), None);
        // The file is a full backup.
        let copy = db::open(&dir.join("wistjet-2026-10-19.db")).unwrap();
        assert_eq!(dump(&copy), dump(&conn));
        assert_eq!(latest(&dir).as_deref(), Some("wistjet-2026-10-19.db"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_the_newest_backups_are_kept_and_other_files_are_left_alone() {
        let dir = a_dir("prune");
        for day in 1..=16 {
            fs::write(dir.join(format!("wistjet-2026-10-{day:02}.db")), b"x").unwrap();
        }
        fs::write(dir.join("notes.txt"), b"mine").unwrap();
        prune(&dir, 14).unwrap();
        let mut left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left.len(), 15);
        assert_eq!(left[0], "notes.txt");
        assert_eq!(left[1], "wistjet-2026-10-03.db");
        fs::remove_dir_all(&dir).unwrap();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib backup::`
Expected: compile errors (`cannot find function backup_bytes`, `cannot find function restore`, …).

- [ ] **Step 3: Implement**

Insert between the doc comment and `#[cfg(test)]` in `src/backup.rs`:
```rust
use crate::db::{self, OpenError};
use crate::family;
use crate::server::{Shared, today};
use chrono::NaiveDate;
use rusqlite::Connection;
use rusqlite::backup::Backup;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, io};

pub const DEFAULT_KEEP: usize = 14;

/// Where automatic backups go and how many are kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupConfig {
    pub dir: PathBuf,
    pub keep: usize,
}

#[derive(Debug)]
pub enum RestoreError {
    /// Not a Wistjet backup (or an empty one, which would wipe the family).
    NotABackup,
    /// Made by a newer Wistjet than this one.
    TooNew,
    Io(io::Error),
    Db(rusqlite::Error),
}

impl From<io::Error> for RestoreError {
    fn from(e: io::Error) -> Self {
        RestoreError::Io(e)
    }
}

impl From<rusqlite::Error> for RestoreError {
    fn from(e: rusqlite::Error) -> Self {
        RestoreError::Db(e)
    }
}

/// A fresh path in the system temp folder.
fn temp_path(what: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("wistjet-{what}-{}-{nanos}.db", std::process::id()))
}

/// Removes a database file and the WAL files SQLite may have left next to it.
fn remove_db_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

/// Writes a consistent copy of the database to `path`, which must not exist yet.
pub fn write_backup(conn: &Connection, path: &Path) -> rusqlite::Result<()> {
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
    Ok(())
}

/// The whole database as one file, for "Download back-up".
// ponytail: built in memory; fine for a family's cards and photos, stream from disk if it ever isn't.
pub fn backup_bytes(conn: &Connection) -> io::Result<Vec<u8>> {
    let path = temp_path("download");
    let bytes = write_backup(conn, &path)
        .map_err(io::Error::other)
        .and_then(|()| fs::read(&path));
    remove_db_files(&path);
    bytes
}

/// Replaces all data in `live` with the backup in `bytes` (spec §9.2). The file is checked
/// and migrated to the current schema in a temporary copy, so an older backup restores into
/// this version and a bad file leaves the live data untouched.
pub fn restore(live: &mut Connection, bytes: &[u8]) -> Result<(), RestoreError> {
    let path = temp_path("restore");
    fs::write(&path, bytes)?;
    let result = (|| {
        let source = db::open(&path).map_err(|e| match e {
            OpenError::TooNew { .. } => RestoreError::TooNew,
            _ => RestoreError::NotABackup,
        })?;
        if !family::is_set_up(&source)? {
            return Err(RestoreError::NotABackup);
        }
        Backup::new(&source, live)?.run_to_completion(256, Duration::ZERO, None)?;
        Ok(())
    })();
    remove_db_files(&path);
    result
}

/// Makes today's automatic backup in `dir` unless it exists, then keeps only the newest
/// `keep` (spec §9.2). Returns the new file, if one was made.
pub fn daily(
    conn: &Connection,
    dir: &Path,
    today: NaiveDate,
    keep: usize,
) -> io::Result<Option<PathBuf>> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("wistjet-{today}.db"));
    let made = if path.exists() {
        None
    } else {
        // Written under another name first, so a half-written file never looks like a backup.
        let partial = dir.join(format!(".wistjet-{today}.db.partial"));
        remove_db_files(&partial);
        write_backup(conn, &partial).map_err(io::Error::other)?;
        fs::rename(&partial, &path)?;
        Some(path)
    };
    prune(dir, keep)?;
    Ok(made)
}

/// Automatic backups in `dir`, oldest first (names sort by date).
fn backups(dir: &Path) -> io::Result<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(dir)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| {
            n.len() == "wistjet-2026-10-19.db".len()
                && n.starts_with("wistjet-")
                && n.ends_with(".db")
        })
        .collect();
    names.sort();
    Ok(names)
}

/// Deletes all but the newest `keep` automatic backups; other files are left alone.
pub fn prune(dir: &Path, keep: usize) -> io::Result<()> {
    let names = backups(dir)?;
    for old in &names[..names.len().saturating_sub(keep)] {
        fs::remove_file(dir.join(old))?;
    }
    Ok(())
}

/// The newest automatic backup's file name, if any.
pub fn latest(dir: &Path) -> Option<String> {
    backups(dir).ok()?.pop()
}

/// Runs for as long as the server does: at start and every 10 minutes, makes today's backup
/// if it isn't there yet (cheap when it is). Frequent enough for a laptop opened briefly.
pub async fn auto_backups(state: Shared, dir: PathBuf, keep: usize) {
    loop {
        {
            let conn = state.db.lock().unwrap();
            if let Ok(Some(day)) = today(&conn)
                && let Err(e) = daily(&conn, &dir, day, keep)
            {
                eprintln!("Automatische back-up mislukt in {}: {e}", dir.display());
            }
        }
        tokio::time::sleep(Duration::from_secs(10 * 60)).await;
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: 6 new tests pass; fmt and clippy clean.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/backup.rs src/lib.rs
git commit -m "feat: backups, restore and daily automatic backups"
```

---

### Task 2: Backup routes and the launcher

**Files:**
- Modify: `src/auth.rs`, `src/server.rs`, `src/parent_api.rs`, `src/main.rs`, `tests/api.rs`

**Interfaces:**
- Consumes: Task 1
- Produces: the API table above; `AppState::new(db, lan_url, hosts, setup_code, backups: Option<BackupConfig>)`; `Auth::end_all_sessions`

- [ ] **Step 1: Write the failing tests**

In `src/auth.rs`, add to the `tests` module before `fn logout_ends_the_session`:
```rust
    #[test]
    fn a_restore_ends_every_session() {
        let mut auth = Auth::default();
        let t0 = Instant::now();
        let t = token(auth.login(t0, || true));
        auth.end_all_sessions();
        assert!(!auth.touch(&t, t0));
    }
```
In `tests/api.rs`, `app_and_state` passes `None` as a new last argument to `server::AppState::new` (after `SETUP_CODE.into(),`), then append:
```rust
// ---- backup and restore (Plan 5a) ----

async fn restore_with(app: &Router, cookie: &str, content_type: &str, bytes: Vec<u8>) -> Reply {
    let req = Request::builder()
        .method("POST")
        .uri("/api/parent/restore")
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    Reply {
        status: res.status(),
        cookie: None,
        content_type: String::new(),
        body: res.into_body().collect().await.unwrap().to_bytes().to_vec(),
    }
}

#[tokio::test]
async fn backup_routes_need_a_parent_session() {
    let app = app();
    set_up(&app).await;
    for (method, path) in [
        ("GET", "/api/parent/backup"),
        ("GET", "/api/parent/backups"),
        ("POST", "/api/parent/restore"),
    ] {
        let r = call(&app, method, path, None, Some(json!({}))).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn download_then_restore_brings_everything_back() {
    let (app, c) = parent().await;
    let r = call(&app, "GET", "/api/parent/backup", Some(&c), None).await;
    assert_eq!(
        (r.status, r.content_type.as_str()),
        (StatusCode::OK, "application/octet-stream")
    );
    assert!(r.body.starts_with(b"SQLite format 3\0"));
    let backup = r.body;

    let child = json!({ "name": "Wout", "avatar": "#4fc3f7" });
    call(&app, "POST", "/api/parent/children", Some(&c), Some(child)).await;
    let r = call(&app, "GET", "/api/children", None, None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 2);

    let r = restore_with(&app, &c, "application/octet-stream", backup).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/children", None, None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 1);
    // The restored family may have another PIN: everyone unlocks again.
    let r = call(&app, "GET", "/api/parent/session", Some(&c), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_download_is_named_after_today() {
    let (app, c) = parent().await;
    let req = Request::builder()
        .uri("/api/parent/backup")
        .header(header::COOKIE, &c)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let disposition = res.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        disposition.starts_with("attachment; filename=\"wistjet-20"),
        "{disposition}"
    );
    assert!(disposition.ends_with(".db\""), "{disposition}");
}

#[tokio::test]
async fn a_bad_restore_is_refused_and_changes_nothing() {
    let (app, c) = parent().await;
    let r = restore_with(&app, &c, "text/plain", b"x".to_vec()).await;
    assert_eq!(r.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let r = restore_with(
        &app,
        &c,
        "application/octet-stream",
        b"not a backup".to_vec(),
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (
            StatusCode::BAD_REQUEST,
            json!({ "error": "invalid", "field": "backup_file" })
        )
    );
    let r = call(&app, "GET", "/api/parent/session", Some(&c), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/children", None, None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn backup_status_without_automatic_backups() {
    let (app, c) = parent().await;
    let r = call(&app, "GET", "/api/parent/backups", Some(&c), None).await;
    assert_eq!(r.json(), json!({ "dir": null, "keep": 0, "latest": null }));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test`
Expected: compile errors (`no method end_all_sessions`; `AppState::new` takes 4 arguments).

- [ ] **Step 3: Implement**

In `src/auth.rs`, add to `impl Auth` before `end_other_sessions`:
```rust
    /// Ends every session (after a restore, the PIN may be a different one).
    pub fn end_all_sessions(&mut self) {
        self.sessions.clear();
    }
```
In `src/server.rs`:
- add `use crate::backup::BackupConfig;`;
- add a field at the end of `AppState`:
```rust
    /// Automatic backups (spec §9.2); `None` in tests.
    pub backups: Option<BackupConfig>,
```
- give `AppState::new` a last parameter `backups: Option<BackupConfig>` and set `backups,` in the struct literal.

In `src/parent_api.rs`, add `use crate::backup::{self, RestoreError};`, add these routes to `routes()` (after `/pin`):
```rust
        .route("/backup", get(download_backup))
        .route("/backups", get(backup_status))
        // A backup holds every photo, so allow a large one.
        .route(
            "/restore",
            post(restore).layer(DefaultBodyLimit::max(1024 * 1024 * 1024)),
        )
```
and append these handlers:
```rust
/// "Download back-up" (spec §9.2): the whole database as one file.
async fn download_backup(State(s): State<Shared>) -> Response {
    let conn = s.db.lock().unwrap();
    let day = match today(&conn) {
        Ok(Some(t)) => t,
        Ok(None) => return error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => return internal(e),
    };
    match backup::backup_bytes(&conn) {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"wistjet-{day}.db\""),
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => internal(e),
    }
}

async fn backup_status(State(s): State<Shared>) -> Response {
    let status = match &s.backups {
        Some(b) => json!({
            "dir": b.dir.display().to_string(),
            "keep": b.keep,
            "latest": backup::latest(&b.dir),
        }),
        None => json!({ "dir": null, "keep": 0, "latest": null }),
    };
    Json(status).into_response()
}

/// Replaces all data with an uploaded backup (spec §9.2), then ends every parent session:
/// the restored family may have another PIN. Only `application/octet-stream` is accepted, so
/// a plain HTML form elsewhere on this host can't trigger a restore.
async fn restore(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let octets = headers
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v == "application/octet-stream");
    if !octets {
        return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "backup_file");
    }
    let mut auth = s.auth.lock().unwrap();
    let result = backup::restore(&mut s.db.lock().unwrap(), &body);
    match result {
        Ok(()) => {
            auth.end_all_sessions();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(RestoreError::NotABackup) => fail(Error::Invalid("backup_file")),
        Err(RestoreError::TooNew) => fail(Error::Invalid("backup_newer")),
        Err(RestoreError::Io(e)) => internal(e),
        Err(RestoreError::Db(e)) => internal(e),
    }
}
```

`src/main.rs` becomes (new: `backup_dir`/`backup_keep` in `Config`, the `BackupConfig` and banner line, the spawned task, the extra `None` in the test helper, and three config test lines):
```rust
//! Launcher: opens the data folder, starts the server on the home network and opens the browser.

use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wistjet::backup::{self, BackupConfig};
use wistjet::{db, server};

#[derive(Debug, PartialEq)]
struct Config {
    /// `None` = the operating system's per-user data folder.
    data_dir: Option<PathBuf>,
    port: u16,
    open_browser: bool,
    /// Extra host names to accept, e.g. a reverse-proxy or ingress name.
    hosts: Vec<String>,
    /// `None` = a `backups` folder inside the data folder.
    backup_dir: Option<PathBuf>,
    backup_keep: usize,
}

impl Config {
    fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let port = match get("WISTJET_PORT") {
            Some(p) => p
                .parse()
                .map_err(|_| format!("WISTJET_PORT is geen geldige poort: {p}"))?,
            None => 8080,
        };
        let backup_keep = match get("WISTJET_BACKUP_KEEP") {
            Some(n) => n
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .ok_or(format!("WISTJET_BACKUP_KEEP is geen geldig aantal: {n}"))?,
            None => backup::DEFAULT_KEEP,
        };
        Ok(Config {
            data_dir: get("WISTJET_DATA_DIR").map(PathBuf::from),
            port,
            open_browser: get("WISTJET_OPEN_BROWSER").is_none_or(|v| v != "0"),
            hosts: get("WISTJET_HOSTS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .map(String::from)
                .collect(),
            backup_dir: get("WISTJET_BACKUP_DIR").map(PathBuf::from),
            backup_keep,
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

    let listener = match bind(config.port).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if !is_wistjet(config.port).await {
                return Err(format!(
                    "poort {} is al in gebruik door een ander programma. \
                     Kies een andere poort met WISTJET_PORT, bijvoorbeeld WISTJET_PORT=8081.",
                    config.port
                )
                .into());
            }
            // Wistjet is already running (double-clicked twice): show it instead of failing.
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

    let set_up = wistjet::family::is_set_up(&conn)?;
    let setup_code = format!("{:06}", rand::random_range(0..1_000_000));
    // Until setup is done, the browser opened here gets the code in its URL; any other
    // device needs it typed in from this window (or `docker logs`).
    let start_url = if set_up {
        local.clone()
    } else {
        format!("{local}/#setup={setup_code}")
    };

    let backups = BackupConfig {
        dir: config
            .backup_dir
            .unwrap_or_else(|| data_dir.join("backups")),
        keep: config.backup_keep,
    };

    println!("Wistjet draait.");
    println!("  Op deze computer:  {local}");
    println!("  Op een tablet:     {lan_url}");
    println!("  Gegevens:          {}", data_dir.display());
    println!("  Back-ups:          {}", backups.dir.display());
    if !set_up {
        println!("  Instelcode:        {setup_code}");
    }
    println!("Sluit dit venster om Wistjet te stoppen.");
    if config.open_browser {
        let _ = open::that(&start_url);
    }
    let state = server::AppState::new(
        conn,
        lan_url,
        config.hosts,
        setup_code,
        Some(backups.clone()),
    );
    tokio::spawn(backup::auto_backups(
        state.clone(),
        backups.dir,
        backups.keep,
    ));
    let app = server::router(state);
    axum::serve(listener, app).await?;
    Ok(())
}

/// Binds the port on all interfaces. Binding loopback first catches a program that holds
/// only 127.0.0.1, which some systems (macOS) let coexist with a 0.0.0.0 bind; that program
/// would then answer `http://localhost` instead of Wistjet.
async fn bind(port: u16) -> std::io::Result<TcpListener> {
    drop(TcpListener::bind(("127.0.0.1", port)).await?);
    TcpListener::bind(("0.0.0.0", port)).await
}

/// True if a Wistjet (an earlier copy that is still running) answers on this port.
async fn is_wistjet(port: u16) -> bool {
    let probe = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
        stream
            .write_all(b"GET /api/status HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .await?;
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).await?;
        Ok::<_, std::io::Error>(String::from_utf8_lossy(&reply).contains("\"setup_done\""))
    };
    matches!(
        tokio::time::timeout(Duration::from_secs(2), probe).await,
        Ok(Ok(true))
    )
}

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
                open_browser: true,
                hosts: vec![],
                backup_dir: None,
                backup_keep: 14
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
                ("WISTJET_HOSTS", " wistjet.thuis, ,flashcards.example.org "),
                ("WISTJET_BACKUP_DIR", "/nas/wistjet"),
                ("WISTJET_BACKUP_KEEP", "30"),
            ]),
            Ok(Config {
                data_dir: Some("/data".into()),
                port: 9000,
                open_browser: false,
                hosts: vec!["wistjet.thuis".into(), "flashcards.example.org".into()],
                backup_dir: Some("/nas/wistjet".into()),
                backup_keep: 30
            })
        );
    }

    #[test]
    fn bad_port_is_an_error() {
        assert!(config(&[("WISTJET_PORT", "tachtig")]).is_err());
        assert!(config(&[("WISTJET_PORT", "70000")]).is_err());
        assert!(config(&[("WISTJET_BACKUP_KEEP", "0")]).is_err());
        assert!(config(&[("WISTJET_BACKUP_KEEP", "veel")]).is_err());
    }

    async fn serve(router: axum::Router) -> u16 {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        port
    }

    #[tokio::test]
    async fn recognises_a_running_wistjet() {
        let conn = wistjet::db::open_in_memory().unwrap();
        let port = serve(server::router(server::AppState::new(
            conn,
            String::new(),
            vec![],
            "123456".into(),
            None,
        )))
        .await;
        assert!(is_wistjet(port).await);
    }

    #[tokio::test]
    async fn another_program_on_the_port_is_not_wistjet() {
        let port = serve(axum::Router::new()).await; // answers every request with 404
        assert!(!is_wistjet(port).await);
    }

    #[tokio::test]
    async fn nothing_on_the_port_is_not_wistjet() {
        let port = std::net::TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port(); // listener dropped: port is closed again
        assert!(!is_wistjet(port).await);
    }

    #[tokio::test]
    async fn program_holding_only_localhost_blocks_the_bind() {
        let other = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = other.local_addr().unwrap().port();
        let err = bind(port).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm --prefix web run build && cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all pass (5 new in `api`, 1 new in `auth`); fmt and clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src tests
git commit -m "feat: backup download, restore and automatic backups in the app"
```

---

### Task 3: Back-up section in Instellingen

**Files:**
- Modify: `web/src/lib/api.ts`, `web/src/lib/messages.ts`, `web/src/lib/types.ts`, `web/src/routes/parent/Settings.svelte`, `web/src/app.css`

**Interfaces:**
- Consumes: Task 2 routes
- Produces: `upload(path, file, contentType?)`, `download(path, fallbackName)`, `BackupStatus`

- [ ] **Step 1: Implement**

`web/src/lib/api.ts` becomes:
```ts
/** Error body the server sends: `{ error, field?, retry_after?, count? }`. */
export type ErrorBody = { error?: string; field?: string; retry_after?: number; count?: number } | null

export class ApiError extends Error {
  constructor(
    public status: number,
    public body: ErrorBody,
  ) {
    super(`HTTP ${status}`)
  }
}

/** GET `/api/<path>`, or send `body` as JSON with `method` (POST unless given). */
export function api<T = unknown>(path: string, body?: unknown, method = 'POST'): Promise<T> {
  const init: RequestInit =
    body === undefined
      ? {}
      : { method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }
  return request<T>(path, init)
}

/** POSTs a file as the raw request body: card images with their own type, a backup with
 *  `application/octet-stream` (the server only accepts that type for a restore). */
export function upload<T = unknown>(path: string, file: Blob, contentType?: string): Promise<T> {
  const headers = contentType ? { 'Content-Type': contentType } : undefined
  return request<T>(path, { method: 'POST', body: file, headers })
}

/** Downloads `/api/<path>` as a file, named the way the server suggests. */
export async function download(path: string, fallbackName: string): Promise<void> {
  const res = await fetch(`/api/${path}`)
  if (res.status === 401 && path.startsWith('parent/')) dispatchEvent(new Event('wistjet:locked'))
  if (!res.ok) throw new ApiError(res.status, null)
  const name = res.headers.get('content-disposition')?.match(/filename="(.+)"/)?.[1] ?? fallbackName
  const url = URL.createObjectURL(await res.blob())
  const link = Object.assign(document.createElement('a'), { href: url, download: name })
  link.click()
  URL.revokeObjectURL(url)
}

async function request<T>(path: string, init: RequestInit): Promise<T> {
  const res = await fetch(`/api/${path}`, init)
  const data = res.headers.get('content-type')?.includes('json') ? await res.json() : null
  // The server ended the parent session (10 idle minutes): back to the PIN screen.
  if (res.status === 401 && path.startsWith('parent/')) dispatchEvent(new Event('wistjet:locked'))
  if (!res.ok) throw new ApiError(res.status, data)
  return data as T
}
```
In `web/src/lib/messages.ts`, add to `FIELD`:
```ts
  backup_file: 'Dat bestand is geen Wistjet-back-up. Er is niets veranderd.',
  backup_newer: 'Deze back-up komt van een nieuwere Wistjet. Werk Wistjet eerst bij. Er is niets veranderd.',
```
Append to `web/src/lib/types.ts`:
```ts
export type BackupStatus = { dir: string | null; keep: number; latest: string | null }
```
`web/src/routes/parent/Settings.svelte` becomes:
```svelte
<script lang="ts">
  import { api, download, upload } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import type { BackupStatus, Settings, SystemDetail } from '../../lib/types'

  let settings = $state<Settings | null>(null)
  let systems = $state<SystemDetail[]>([])
  let zones = $state<string[]>([])
  let pins = $state({ current: '', new: '', again: '' })
  let status = $state({ settings: '', pin: '', backup: '' })
  let errors = $state({ settings: '', pin: '', backup: '' })
  let backups = $state<BackupStatus | null>(null)
  let busy = $state(false)

  async function load() {
    ;[settings, systems] = await Promise.all([api<Settings>('parent/settings'), api<SystemDetail[]>('parent/systems')])
    zones = Intl.supportedValuesOf('timeZone')
    if (!zones.includes(settings.timezone)) zones = [settings.timezone, ...zones]
  }
  load().catch(() => (errors.settings = 'De instellingen konden niet geladen worden.'))
  api<BackupStatus>('parent/backups').then((b) => (backups = b), () => {})

  async function run(section: 'settings' | 'pin' | 'backup', action: () => Promise<unknown>, done: string) {
    if (busy) return
    busy = true
    errors[section] = ''
    status[section] = ''
    try {
      await action()
      status[section] = done
    } catch (err) {
      errors[section] = problem(err)
    } finally {
      busy = false
    }
  }

  function save(e: SubmitEvent) {
    e.preventDefault()
    if (settings) run('settings', () => api('parent/settings', settings, 'PUT'), 'Bewaard.')
  }

  function changePin(e: SubmitEvent) {
    e.preventDefault()
    if (pins.new !== pins.again) {
      errors.pin = 'De twee nieuwe pincodes zijn niet gelijk.'
      return
    }
    run(
      'pin',
      async () => {
        await api('parent/pin', { current: pins.current, new: pins.new })
        pins = { current: '', new: '', again: '' }
      },
      'De pincode is gewijzigd. Andere toestellen moeten opnieuw ontgrendelen.',
    )
  }

  function downloadBackup() {
    run('backup', () => download('parent/backup', 'wistjet-back-up.db'), 'De back-up is gedownload.')
  }

  async function restore(e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    if (!confirm(`Alle huidige gegevens vervangen door "${file.name}"? Dit kan niet ongedaan gemaakt worden.`)) return
    await run('backup', () => upload('parent/restore', file, 'application/octet-stream'), '')
    if (!errors.backup) {
      alert('De back-up is teruggezet. Ontgrendel opnieuw met de pincode uit de back-up.')
      dispatchEvent(new Event('wistjet:locked'))
    }
  }
</script>

{#if settings}
  <section class="card">
    <h2>Gezin</h2>
    <form onsubmit={save}>
      <label>Familienaam <input bind:value={settings.name} required /></label>
      <label>
        Tijdzone
        <select bind:value={settings.timezone}>
          {#each zones as zone (zone)}<option>{zone}</option>{/each}
        </select>
      </label>
      <label>
        Standaard studiesysteem voor nieuwe decks
        <select bind:value={settings.default_system_id}>
          {#each systems as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
        </select>
      </label>
      {#if errors.settings}<p class="error" role="alert">{errors.settings}</p>{/if}
      {#if status.settings}<p class="saved" role="status">{status.settings}</p>{/if}
      <div class="actions"><button type="submit" disabled={busy}>Bewaar</button></div>
    </form>
  </section>
{:else if errors.settings}
  <p class="error" role="alert">{errors.settings}</p>
{/if}

<section class="card">
  <h2>Pincode wijzigen</h2>
  <form onsubmit={changePin}>
    <label>
      Huidige pincode
      <input type="password" inputmode="numeric" autocomplete="current-password" bind:value={pins.current} required />
    </label>
    <label>
      Nieuwe pincode (4 tot 8 cijfers)
      <input type="password" inputmode="numeric" autocomplete="new-password" bind:value={pins.new} required />
    </label>
    <label>
      Herhaal de nieuwe pincode
      <input type="password" inputmode="numeric" autocomplete="new-password" bind:value={pins.again} required />
    </label>
    {#if errors.pin}<p class="error" role="alert">{errors.pin}</p>{/if}
    {#if status.pin}<p class="saved" role="status">{status.pin}</p>{/if}
    <div class="actions"><button type="submit" disabled={busy}>Wijzig pincode</button></div>
  </form>
</section>

<section class="card">
  <h2>Back-up</h2>
  <p>Eén bestand met alles: kinderen, decks, kaartjes, afbeeldingen, voortgang en instellingen.</p>
  {#if backups?.dir}
    <p class="muted">
      Elke dag maakt Wistjet zelf een back-up in <code>{backups.dir}</code> en bewaart er de laatste
      {backups.keep}.{#if backups.latest} Laatste: {backups.latest}.{/if}
    </p>
  {/if}
  {#if errors.backup}<p class="error" role="alert">{errors.backup}</p>{/if}
  {#if status.backup}<p class="saved" role="status">{status.backup}</p>{/if}
  <div class="actions">
    <label class="button secondary file-button">
      Back-up terugzetten…
      <input type="file" accept=".db,application/octet-stream,application/vnd.sqlite3" onchange={restore} disabled={busy} />
    </label>
    <button onclick={downloadBackup} disabled={busy}>Download back-up</button>
  </div>
</section>
```
Append to `web/src/app.css`:
```css
.file-button {
  margin: 0;
  position: relative;
}

.file-button input {
  position: absolute;
  opacity: 0;
  width: 1px;
  height: 1px;
}

.file-button:has(input:focus-visible) {
  outline: 3px solid color-mix(in srgb, var(--accent) 50%, transparent);
  outline-offset: 2px;
}

code {
  overflow-wrap: anywhere;
}
```

- [ ] **Step 2: Check and build**

Run: `npm --prefix web run check && npm --prefix web test && npm --prefix web run build`
Expected: svelte-check 0 errors, 0 warnings; tests pass; build succeeds.

- [ ] **Step 3: Look at it**

Release binary, fresh data folder, setup with the sample deck, unlock, Instellingen:
- **Automatic backups:** the Back-up section names the folder and "de laatste 14".
- **Download:** "Download back-up" saves `wistjet-<today>.db`, and the page says "De back-up is gedownload."
- **Restore:** add a child, then "Back-up terugzetten…" with that file. Confirm, then OK on the message: the PIN screen appears, and the child you added is gone again.
- **A bad file:** restoring any other file says it isn't a Wistjet backup, and nothing changes.
- **Restart:** stop and start the app. `backups/wistjet-<today>.db` now exists in the data folder, and the section shows it as the latest.

- [ ] **Step 4: Commit**

```bash
git add web/src
git commit -m "feat: back-up section in Instellingen"
```

---

### Task 4: End-to-end test

**Files:**
- Create: `e2e/e2e.mjs`
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Write the test**

Create `e2e/e2e.mjs`:
```js
// End-to-end test (spec §11): first run → create a deck by pasting a list → study a
// session → summary, through the real web app in headless Chrome. No dependencies: Node 22
// (built-in fetch and WebSocket) drives Chrome over the DevTools protocol.
//
//   node e2e/e2e.mjs target/release/wistjet
//
// Set CHROME to the browser binary if it isn't found. Exits 1 on the first failure.

import { spawn } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const bin = process.argv[2] ?? 'target/release/wistjet'
const chromeBin =
  process.env.CHROME ??
  (process.platform === 'darwin'
    ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
    : 'google-chrome')
const PORT = 18123
const DEVTOOLS = 9411
const BASE = `http://localhost:${PORT}`
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let server
let chrome
function stop(code) {
  server?.kill()
  chrome?.kill()
  process.exit(code)
}
function check(ok, what) {
  console.log(`${ok ? 'PASS' : 'FAIL'} ${what}`)
  if (!ok) stop(1)
}
setTimeout(() => {
  console.log('FAIL timed out')
  stop(1)
}, 120_000)

// ---- the app ----
server = spawn(bin, [], {
  env: {
    ...process.env,
    WISTJET_DATA_DIR: mkdtempSync(join(tmpdir(), 'wistjet-e2e-')),
    WISTJET_PORT: String(PORT),
    WISTJET_OPEN_BROWSER: '0',
  },
  stdio: ['ignore', 'pipe', 'inherit'],
})
let banner = ''
server.stdout.on('data', (d) => (banner += d))
for (let i = 0; i < 100 && !banner.includes('Sluit dit venster'); i++) await sleep(100)
const code = banner.match(/Instelcode:\s+(\d{6})/)?.[1]
check(Boolean(code), 'the app starts and prints a setup code')

// ---- the browser ----
chrome = spawn(
  chromeBin,
  [
    '--headless=new',
    '--disable-gpu',
    '--no-sandbox',
    `--remote-debugging-port=${DEVTOOLS}`,
    `--user-data-dir=${mkdtempSync(join(tmpdir(), 'wistjet-e2e-chrome-'))}`,
    '--window-size=1000,800',
    'about:blank',
  ],
  { stdio: 'ignore' },
)
let targets = []
for (let i = 0; i < 50 && !targets.some((t) => t.type === 'page'); i++) {
  await sleep(200)
  targets = await fetch(`http://127.0.0.1:${DEVTOOLS}/json`).then((r) => r.json(), () => [])
}
const ws = new WebSocket(targets.find((t) => t.type === 'page').webSocketDebuggerUrl)
await new Promise((r) => ws.addEventListener('open', r))
let nextId = 0
const pending = new Map()
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data)
  if (m.id && pending.has(m.id)) pending.get(m.id)(m)
  if (m.method === 'Page.javascriptDialogOpening') cdp('Page.handleJavaScriptDialog', { accept: true })
})
function cdp(method, params = {}) {
  return new Promise((resolve) => {
    const id = ++nextId
    pending.set(id, resolve)
    ws.send(JSON.stringify({ id, method, params }))
  })
}
const js = async (expression) =>
  (await cdp('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result.result?.value
const text = async () => (await js('document.body.innerText')) ?? ''
const go = async (url) => {
  await cdp('Page.navigate', { url })
  await sleep(1000)
}
/** Types into the nth element matching `selector`. */
async function type(selector, value, nth = 0) {
  await js(`document.querySelectorAll(${JSON.stringify(selector)})[${nth}].focus()`)
  await cdp('Input.insertText', { text: value })
}
/** Clicks the button or link whose text is exactly `label`. */
async function click(label) {
  const found = await js(`(() => {
    const el = [...document.querySelectorAll('button, a')].find((b) => b.textContent.trim() === ${JSON.stringify(label)})
    el?.click()
    return Boolean(el)
  })()`)
  check(found, `found "${label}"`)
  await sleep(500)
}
async function key(k) {
  const codes = { ' ': ['Space', 32], ArrowLeft: ['ArrowLeft', 37], ArrowRight: ['ArrowRight', 39] }
  for (const t of ['keyDown', 'keyUp'])
    await cdp('Input.dispatchKeyEvent', { type: t, key: k, code: codes[k][0], windowsVirtualKeyCode: codes[k][1] })
  await sleep(300)
}
await cdp('Page.enable')

// ---- first run: the browser opened by the app gets the code in its address ----
await go(`${BASE}/#setup=${code}`)
check((await text()).includes('Welkom bij Wistjet'), 'first run shows the setup wizard')
await type('input', 'De Smet')
await type('input[type=password]', '2468', 0)
await type('input[type=password]', '2468', 1)
await click('Volgende')
await type('input', 'Lotte')
await click('Volgende')
await js(`document.querySelectorAll('input[name=sample]')[1].click()`) // start empty
await click('Klaar')
check((await text()).includes('Klaar!'), 'setup finishes')
await click('Start')
check((await text()).includes('Wie gaat er oefenen?') && (await text()).includes('Lotte'), 'the profile screen shows the child')

// ---- the parent creates a subject and a deck by pasting a list ----
await click('🔒')
await type('input[type=password]', '2468')
await click('Ontgrendel')
check((await text()).includes('Overzicht'), 'the PIN opens the parent area')
await go(`${BASE}/#/ouder/vakken`)
await type('section.card input', 'Frans')
await click('Toevoegen')
await go(`${BASE}/#/ouder/decks`)
await type('section.card input', 'Woordjes H1')
await click('Toevoegen')
check((await text()).includes('Kaartjes (0)'), 'a new deck opens on its own page')
await type('textarea', 'le chat - de kat\nle chien - de hond\nla maison - het huis')
await click('Bekijk')
await click('Voeg 3 kaartjes toe')
check((await text()).includes('Kaartjes (3)'), 'pasting a list creates 3 cards')
await click('Vergrendel')

// ---- the child studies ----
await go(`${BASE}/#/`)
await js(`document.querySelector('a.tile').click()`)
await sleep(800)
check((await text()).includes('3 kaartjes om te oefenen'), 'the child sees 3 cards due')
await click('Start met oefenen')
check((await text()).includes('1 van 3'), 'the session starts at 1 van 3')
await key(' ')
await key('ArrowLeft') // not known: comes back at the end
check((await text()).includes('2 van 4'), 'a missed card comes back at the end')
for (let i = 0; i < 3; i++) {
  await key(' ')
  await key('ArrowRight')
}
const summary = (await text()).replace(/\s+/g, ' ')
check(summary.includes('Goed gedaan!'), 'the summary appears')
check(/2 wist je 1 nog niet 1 keer herhaald/.test(summary), `the summary counts known, not known and repeats (${summary})`)

console.log('All end-to-end checks passed.')
stop(0)
```

- [ ] **Step 2: Run it**

Run: `npm --prefix web run build && cargo build --release && node e2e/e2e.mjs target/release/wistjet`
Expected: 24 `PASS` lines and "All end-to-end checks passed."; exit code 0. To see it fail, break something on purpose (e.g. change the summary's "wist je"): it must stop with a `FAIL` line and exit code 1.

- [ ] **Step 3: Run it in CI**

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
      - run: npm --prefix web test
      - run: npm --prefix web run build
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test
      # Spec §11's end-to-end test, in the Chrome that GitHub's Ubuntu runners include.
      - run: cargo build --release
      - run: node e2e/e2e.mjs target/release/wistjet
```

- [ ] **Step 4: Commit**

```bash
git add e2e .github/workflows/ci.yml
git commit -m "test: end-to-end test of first run, paste, study and summary"
```

---

## Done when

- All Rust and web tests pass (5a adds 7 library, 5 API and 3 launcher config assertions), the web check and build are clean, and `node e2e/e2e.mjs` passes locally and in CI.
- The Task 3 click-through behaves as listed.
- Spec coverage: §9.2 complete (download, restore with confirmation, older backups upgrade on restore, daily automatic backups, configurable folder and count); §10 invalid restore; §11 backup round trip and the end-to-end test. Plan 5b: §9.1 (one-command install on a computer and on a home cluster, README).
