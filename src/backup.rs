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
    let dir = scratch()?;
    let path = dir.path().join("backup.db");
    write_backup(conn, &path).map_err(io::Error::other)?;
    fs::read(&path)
}

/// Replaces all data in `live` with the backup in `bytes` (spec §9.2). The file is checked
/// and migrated to the current schema in a temporary copy, so an older backup restores into
/// this version and a bad file leaves the live data untouched.
pub fn restore(live: &mut Connection, bytes: &[u8]) -> Result<(), RestoreError> {
    let dir = scratch()?; // deleted, with the copy in it, when this function returns
    let path = dir.path().join("restore.db");
    fs::write(&path, bytes)?;
    let source = db::open(&path).map_err(|e| match e {
        OpenError::TooNew { .. } => RestoreError::TooNew,
        _ => RestoreError::NotABackup,
    })?;
    if !family::is_set_up(&source)? {
        return Err(RestoreError::NotABackup);
    }
    Backup::new(&source, live)?.run_to_completion(256, Duration::ZERO, None)?;
    Ok(())
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
        let conn = family();
        let tmp = scratch().unwrap();
        let dir = tmp.path();
        let made = daily(&conn, dir, d("2026-10-19"), 14).unwrap();
        assert_eq!(made, Some(dir.join("wistjet-2026-10-19.db")));
        assert_eq!(daily(&conn, dir, d("2026-10-19"), 14).unwrap(), None);
        // The file is a full backup.
        let copy = db::open(&dir.join("wistjet-2026-10-19.db")).unwrap();
        assert_eq!(dump(&copy), dump(&conn));
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
