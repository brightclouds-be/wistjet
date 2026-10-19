//! Backups (spec §9.2). A backup is the SQLite database itself: content, images, progress,
//! review log and settings in one file. Restoring checks and upgrades the file in a temporary
//! copy first and only then copies it into the live database, so a bad file changes nothing.

use crate::db::{self, OpenError};
use crate::family;
use crate::server::{Shared, today};
use chrono::NaiveDate;
use rusqlite::Connection;
use rusqlite::backup::Backup;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
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

/// A private folder (only this user can open it, `0700` on Unix) for one backup or restore,
/// deleted when dropped. A predictable name in the shared temp folder would let other users
/// on the machine read the family's data or plant a link where the file is written.
fn scratch() -> io::Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("wistjet-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    builder.tempdir()
}

/// Writes a consistent copy of the database to `path`, which must not exist yet.
pub fn write_backup(conn: &Connection, path: &Path) -> rusqlite::Result<()> {
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
    Ok(())
}

/// The whole database as one file, for "Download back-up".
// ponytail: built in memory; fine for a family's cards and photos, stream from disk if it ever isn't.
pub fn backup_bytes(conn: &Connection) -> io::Result<Vec<u8>> {
    let dir = scratch()?;
    let path = dir.path().join("backup.db");
    write_backup(conn, &path).map_err(io::Error::other)?;
    fs::read(&path)
}

/// A checked backup, rebuilt in this version's own tables, ready to replace the live data.
pub struct Prepared {
    /// Keeps the private folder (and the files in it) until the backup is applied.
    _dir: tempfile::TempDir,
    clean: Connection,
}

/// Checks a backup and rebuilds it (spec §9.2, §10). The upload must be Wistjet's and not
/// from a newer version; an older one is migrated. Its rows are then copied into a fresh
/// database this version built, so every table definition, CHECK and foreign key is ours,
/// however the upload words its own SQL (older releases may differ, a tampered file may
/// leave a CHECK out). Slow, and needs no lock: nothing live is touched.
pub fn prepare(bytes: &[u8]) -> Result<Prepared, RestoreError> {
    let dir = scratch()?;
    let upload = dir.path().join("upload.db");
    fs::write(&upload, bytes)?;
    drop(db::open(&upload).map_err(|e| match e {
        OpenError::TooNew { .. } => RestoreError::TooNew,
        _ => RestoreError::NotABackup,
    })?);
    let mut clean = db::open(&dir.path().join("clean.db"))
        .map_err(|e| RestoreError::Io(io::Error::other(format!("{e:?}"))))?;
    copy_rows(&mut clean, &upload).map_err(|_| RestoreError::NotABackup)?;
    if !family::is_set_up(&clean)? || !trustworthy(&clean)? {
        return Err(RestoreError::NotABackup);
    }
    Ok(Prepared { _dir: dir, clean })
}

/// Replaces all live data with a prepared backup in one atomic copy: readers see the old
/// data or the new, never a mix, and a failure halfway changes nothing.
pub fn apply(prepared: &Prepared, live: &mut Connection) -> Result<(), RestoreError> {
    Backup::new(&prepared.clean, live)?.run_to_completion(256, Duration::ZERO, None)?;
    Ok(())
}

/// `prepare` then `apply`, for callers that hold no locks anyway.
pub fn restore(live: &mut Connection, bytes: &[u8]) -> Result<(), RestoreError> {
    apply(&prepare(bytes)?, live)
}

/// Copies every table's rows from `upload` into `clean`'s own tables. Table and column names
/// come from `clean`'s schema, never from the upload.
fn copy_rows(clean: &mut Connection, upload: &Path) -> rusqlite::Result<()> {
    clean.execute_batch("PRAGMA foreign_keys = OFF")?; // rows arrive in any order; checked after
    clean.execute("ATTACH DATABASE ?1 AS upload", [upload.to_string_lossy()])?;
    let tables: Vec<String> = clean
        .prepare(
            "SELECT name FROM main.sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let tx = clean.transaction()?;
    for table in &tables {
        let columns: Vec<String> = tx
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let columns = columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        tx.execute(&format!("DELETE FROM main.\"{table}\""), [])?;
        tx.execute(
            &format!(
                "INSERT INTO main.\"{table}\" ({columns}) SELECT {columns} FROM upload.\"{table}\""
            ),
            [],
        )?;
    }
    tx.commit()?;
    clean.execute("DETACH DATABASE upload", [])?;
    clean.execute_batch("PRAGMA foreign_keys = ON")
}

/// No damage, no broken references, and settings the family can still use: a timezone this
/// version knows and a PIN hash it can check (otherwise nobody could unlock to restore again).
fn trustworthy(conn: &Connection) -> Result<bool, RestoreError> {
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    let broken_reference = conn.prepare("PRAGMA foreign_key_check")?.exists([])?;
    let (timezone, pin_hash): (String, String) =
        conn.query_row("SELECT timezone, pin_hash FROM family", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    Ok(integrity == "ok"
        && !broken_reference
        && timezone.parse::<chrono_tz::Tz>().is_ok()
        && argon2::password_hash::PasswordHash::new(&pin_hash).is_ok())
}

/// Makes today's automatic backup in `dir` unless it exists, then keeps only the newest
/// `keep` (spec §9.2). Only a quick local snapshot holds the database lock; the copy into
/// `dir` (perhaps a slow or stalled NAS share) happens without it, so children keep
/// studying meanwhile. Returns the new file, if one was made.
pub fn daily(
    db: &Mutex<Connection>,
    dir: &Path,
    today: NaiveDate,
    keep: usize,
) -> io::Result<Option<PathBuf>> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("wistjet-{today}.db"));
    let made = if path.exists() {
        None
    } else {
        let tmp = scratch()?;
        let snapshot = tmp.path().join("snapshot.db");
        write_backup(&db.lock().unwrap(), &snapshot).map_err(io::Error::other)?;
        // Copied under another name first, so a half-written file never looks like a backup.
        let partial = dir.join(format!(".wistjet-{today}.db.partial"));
        let _ = fs::remove_file(&partial);
        fs::copy(&snapshot, &partial)?;
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
            n.strip_prefix("wistjet-")
                .and_then(|rest| rest.strip_suffix(".db"))
                .is_some_and(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok())
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
        let day = today(&state.db.lock().unwrap());
        if let Ok(Some(day)) = day {
            let (state, target) = (state.clone(), dir.clone());
            // File work, maybe on a NAS, runs on a blocking thread, not on the async workers.
            let made =
                tokio::task::spawn_blocking(move || daily(&state.db, &target, day, keep)).await;
            if let Ok(Err(e)) = made {
                eprintln!("Automatische back-up mislukt in {}: {e}", dir.display());
            }
        }
        tokio::time::sleep(Duration::from_secs(10 * 60)).await;
    }
}

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

    #[cfg(unix)]
    #[test]
    fn scratch_space_is_private_to_this_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch().unwrap();
        let mode = fs::metadata(dir.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
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
            let tmp = scratch().unwrap();
            let path = tmp.path().join("other.db");
            Connection::open(&path)
                .unwrap()
                .execute_batch("CREATE TABLE notes (x)")
                .unwrap();
            fs::read(&path).unwrap()
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

    /// A backup of `family()`, changed by `sql` with constraints and foreign keys off.
    fn tampered(sql: &str) -> Vec<u8> {
        let tmp = scratch().unwrap();
        let path = tmp.path().join("tampered.db");
        write_backup(&family(), &path).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute_batch(&format!("PRAGMA foreign_keys = OFF; {sql}"))
            .unwrap();
        fs::read(&path).unwrap()
    }

    #[test]
    fn a_backup_with_its_own_table_definitions_is_refused() {
        // Without the CHECK on image types, a backup could carry an HTML "image".
        let bytes = tampered(
            "CREATE TABLE image2 (id INTEGER PRIMARY KEY, mime TEXT NOT NULL, data BLOB NOT NULL);
             INSERT INTO image2 VALUES (9, 'text/html', '<script>alert(1)</script>');
             DROP TABLE image;
             ALTER TABLE image2 RENAME TO image;",
        );
        let mut conn = family();
        let before = dump(&conn);
        assert!(matches!(
            restore(&mut conn, &bytes),
            Err(RestoreError::NotABackup)
        ));
        assert_eq!(dump(&conn), before);
    }

    #[test]
    fn a_backup_with_broken_references_is_refused() {
        let bytes =
            tampered("INSERT INTO card (deck_id, front_text, back_text) VALUES (999, 'x', 'y');");
        let mut conn = family();
        assert!(matches!(
            restore(&mut conn, &bytes),
            Err(RestoreError::NotABackup)
        ));
    }

    #[test]
    fn a_backup_whose_table_text_differs_but_whose_data_is_valid_restores() {
        // An older release may have written the same tables with different SQL text
        // (comments, spacing). Only the data has to fit this version's tables.
        let bytes = tampered(
            "CREATE TABLE subject2 (id INTEGER PRIMARY KEY,   name TEXT NOT NULL, -- reformatted
                                    archived INTEGER NOT NULL DEFAULT 0);
             INSERT INTO subject2 SELECT * FROM subject;
             DROP TABLE subject;
             ALTER TABLE subject2 RENAME TO subject;",
        );
        let mut conn = db::open_in_memory().unwrap();
        restore(&mut conn, &bytes).unwrap();
        let subjects: i64 = conn
            .query_row("SELECT count(*) FROM subject", [], |r| r.get(0))
            .unwrap();
        assert_eq!(subjects, 1);
    }

    #[test]
    fn a_backup_that_would_lock_the_family_out_is_refused() {
        for sql in [
            "UPDATE family SET timezone = 'Mars/Olympus'",
            "UPDATE family SET pin_hash = 'garbage'",
        ] {
            let mut conn = family();
            assert!(
                matches!(
                    restore(&mut conn, &tampered(sql)),
                    Err(RestoreError::NotABackup)
                ),
                "{sql}"
            );
        }
    }

    #[test]
    fn only_dated_automatic_backups_are_pruned() {
        let tmp = scratch().unwrap();
        let dir = tmp.path();
        for day in 1..=3 {
            fs::write(dir.join(format!("wistjet-2026-10-{day:02}.db")), b"x").unwrap();
        }
        // A parent's own downloads, and a file that only looks like a backup.
        fs::write(dir.join("wistjet-back-up-2026-09-01.db"), b"mine").unwrap();
        fs::write(dir.join("wistjet-notadate12.db"), b"mine").unwrap();
        prune(dir, 1).unwrap();
        let mut left: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "wistjet-2026-10-03.db",
                "wistjet-back-up-2026-09-01.db",
                "wistjet-notadate12.db"
            ]
        );
    }

    #[test]
    fn a_backup_from_a_newer_version_is_refused() {
        let mut conn = family();
        let tmp = scratch().unwrap();
        let path = tmp.path().join("newer.db");
        write_backup(&conn, &path).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute_batch("PRAGMA user_version = 99")
            .unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(matches!(
            restore(&mut conn, &bytes),
            Err(RestoreError::TooNew)
        ));
    }

    #[test]
    fn one_automatic_backup_per_day() {
        let tmp = scratch().unwrap();
        let dir = tmp.path();
        let db = Mutex::new(family());
        let made = daily(&db, dir, d("2026-10-19"), 14).unwrap();
        assert_eq!(made, Some(dir.join("wistjet-2026-10-19.db")));
        assert_eq!(daily(&db, dir, d("2026-10-19"), 14).unwrap(), None);
        // The file is a full backup.
        let copy = db::open(&dir.join("wistjet-2026-10-19.db")).unwrap();
        assert_eq!(dump(&copy), dump(&db.lock().unwrap()));
        assert_eq!(latest(dir).as_deref(), Some("wistjet-2026-10-19.db"));
    }

    #[test]
    fn only_the_newest_backups_are_kept_and_other_files_are_left_alone() {
        let tmp = scratch().unwrap();
        let dir = tmp.path();
        for day in 1..=16 {
            fs::write(dir.join(format!("wistjet-2026-10-{day:02}.db")), b"x").unwrap();
        }
        fs::write(dir.join("notes.txt"), b"mine").unwrap();
        prune(dir, 14).unwrap();
        let mut left: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left.len(), 15);
        assert_eq!(left[0], "notes.txt");
        assert_eq!(left[1], "wistjet-2026-10-03.db");
    }
}
