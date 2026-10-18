//! The one family per installation: first-run setup (spec §7.8) and the lookups other code needs.

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
