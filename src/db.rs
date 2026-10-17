//! Opening the SQLite file and applying migrations (spec §9.1: upgrades migrate automatically).

use rusqlite::Connection;
use std::path::Path;

const MIGRATIONS: &[&str] = &[include_str!("../migrations/001_init.sql")];
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
pub fn prepare(mut conn: Connection) -> Result<Connection, OpenError> {
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let app_id: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    if version > 0 && app_id != APPLICATION_ID {
        return Err(OpenError::NotWistjet);
    }
    if version > MIGRATIONS.len() {
        return Err(OpenError::TooNew {
            found: version,
            supported: MIGRATIONS.len(),
        });
    }
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
}
