//! Opening the SQLite file and applying migrations (spec §9.1: upgrades migrate automatically).

use rusqlite::Connection;
use std::path::Path;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_init.sql"),
    include_str!("../migrations/002_tables.sql"),
];
const APPLICATION_ID: i64 = 1464423252; // "WIST", set by migration 001

#[derive(Debug)]
pub enum OpenError {
    Sqlite(rusqlite::Error),
    NotWistjet,
    TooNew { found: usize, supported: usize },
}

impl From<rusqlite::Error> for OpenError {
    fn from(e: rusqlite::Error) -> Self {
        OpenError::Sqlite(e)
    }
}

pub fn open(path: &Path) -> Result<Connection, OpenError> {
    prepare(Connection::open(path)?)
}

pub fn open_in_memory() -> Result<Connection, OpenError> {
    prepare(Connection::open_in_memory()?)
}

/// Checks the file is ours and not from a newer app, then migrates it to the latest schema.
/// A rejected file is left exactly as it was: the checks only read.
pub fn prepare(mut conn: Connection) -> Result<Connection, OpenError> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let app_id: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    let tables: i64 = conn.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))?;
    // An unversioned file is only ours to initialise if it is completely empty.
    let ours = if version == 0 {
        app_id == 0 && tables == 0
    } else {
        app_id == APPLICATION_ID
    };
    if !ours {
        return Err(OpenError::NotWistjet);
    }
    if version > MIGRATIONS.len() {
        return Err(OpenError::TooNew {
            found: version,
            supported: MIGRATIONS.len(),
        });
    }
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_gets_schema_and_preset() {
        let conn = open_in_memory().unwrap();
        let boxes: i64 = conn
            .query_row("SELECT count(*) FROM box WHERE system_id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(boxes, 4);
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let conn = prepare(open_in_memory().unwrap()).unwrap();
        let systems: i64 = conn
            .query_row("SELECT count(*) FROM study_system", [], |r| r.get(0))
            .unwrap();
        assert_eq!(systems, 1);
    }

    #[test]
    fn version_1_files_migrate_to_self_decks_and_3_second_children() {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute_batch(MIGRATIONS[0]).unwrap();
        tx.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO child (name, avatar) VALUES ('Lotte', '#e57373');
             INSERT INTO subject (name) VALUES ('Frans');
             INSERT INTO deck (subject_id, name, system_id, direction_mode)
             VALUES (1, 'H1', 1, 'both');",
        )
        .unwrap();
        tx.commit().unwrap();
        let conn = prepare(conn).unwrap();
        let row: (String, i64) = conn
            .query_row(
                "SELECT d.answer_mode, c.time_limit_ms FROM deck d, child c",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("self".into(), 3000));
    }

    #[test]
    fn rejects_database_from_newer_app() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA application_id = 1464423252; PRAGMA user_version = 99;")
            .unwrap();
        assert!(matches!(
            prepare(conn),
            Err(OpenError::TooNew { found: 99, .. })
        ));
    }

    #[test]
    fn rejects_foreign_sqlite_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 3;").unwrap();
        assert!(matches!(prepare(conn), Err(OpenError::NotWistjet)));
    }

    #[test]
    fn rejects_file_that_is_not_sqlite() {
        let path = std::env::temp_dir().join(format!("wistjet-garbage-{}.db", std::process::id()));
        std::fs::write(
            &path,
            b"this is not a database, just some text padding it out",
        )
        .unwrap();
        let result = open(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(result, Err(OpenError::Sqlite(_))));
    }

    #[test]
    fn rejects_foreign_database_without_user_version() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY);")
            .unwrap();
        assert!(matches!(prepare(conn), Err(OpenError::NotWistjet)));
    }

    #[test]
    fn rejected_file_is_left_untouched() {
        let path = std::env::temp_dir().join(format!("wistjet-foreign-{}.db", std::process::id()));
        Connection::open(&path)
            .unwrap()
            .execute_batch("PRAGMA user_version = 3; CREATE TABLE notes (id INTEGER);")
            .unwrap();
        let result = open(&path);
        let mode: String = Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        assert!(matches!(result, Err(OpenError::NotWistjet)));
        assert_eq!(mode, "delete");
    }
}
