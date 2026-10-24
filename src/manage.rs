//! What the parent manages (spec §7.2–7.4): children, subjects, decks, cards and images.
//! Nothing here deletes: archiving and deactivating hide things and keep progress (§5.8).

use crate::auth;
use crate::family::{clean_name, valid_avatar};
use crate::store::{self, VISIBLE};
use chrono::NaiveDate;
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, named_params, params};
use serde::{Deserialize, Serialize};

const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    /// Names the offending field, as sent by the client.
    Invalid(&'static str),
    NotFound,
    /// Not allowed in this state, with a count where it helps explain why
    /// (e.g. `("in_use", 3)`: a study system used by 3 decks).
    Refused(&'static str, i64),
    Db(rusqlite::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn exists(conn: &Connection, sql: &str, id: i64) -> Result<bool> {
    Ok(conn.query_row(sql, [id], |r| r.get(0))?)
}

fn found(changed: usize) -> Result<()> {
    if changed == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

// ---- children ----

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ChildRow {
    pub id: i64,
    pub name: String,
    pub avatar: String,
    pub archived: bool,
    /// Time allowed per answer on "Typen op tijd" decks (tables spec §5).
    pub time_limit_ms: i64,
}

#[derive(Deserialize)]
pub struct ChildInput {
    pub name: String,
    pub avatar: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default = "default_time_limit")]
    pub time_limit_ms: i64,
}

fn default_time_limit() -> i64 {
    3000
}

fn check_child(c: &ChildInput) -> Result<String> {
    let name = clean_name(&c.name).ok_or(Error::Invalid("name"))?;
    if !valid_avatar(&c.avatar) {
        return Err(Error::Invalid("avatar"));
    }
    if !(1000..=30000).contains(&c.time_limit_ms) || c.time_limit_ms % 500 != 0 {
        return Err(Error::Invalid("time_limit_ms"));
    }
    Ok(name)
}

/// All children, archived ones too, in the order they were added.
pub fn list_children(conn: &Connection) -> Result<Vec<ChildRow>> {
    let mut stmt =
        conn.prepare("SELECT id, name, avatar, archived, time_limit_ms FROM child ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok(ChildRow {
            id: r.get(0)?,
            name: r.get(1)?,
            avatar: r.get(2)?,
            archived: r.get(3)?,
            time_limit_ms: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn create_child(conn: &Connection, c: &ChildInput) -> Result<i64> {
    let name = check_child(c)?;
    conn.execute(
        "INSERT INTO child (name, avatar, archived, time_limit_ms) VALUES (?1, ?2, ?3, ?4)",
        params![name, c.avatar, c.archived, c.time_limit_ms],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Renames, recolours, archives or restores a child. Progress is never touched.
pub fn update_child(conn: &Connection, id: i64, c: &ChildInput) -> Result<()> {
    let name = check_child(c)?;
    found(conn.execute(
        "UPDATE child SET name = ?2, avatar = ?3, archived = ?4, time_limit_ms = ?5 WHERE id = ?1",
        params![id, name, c.avatar, c.archived, c.time_limit_ms],
    )?)
}

// ---- subjects ----

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SubjectRow {
    pub id: i64,
    pub name: String,
    pub archived: bool,
}

#[derive(Deserialize)]
pub struct SubjectInput {
    pub name: String,
    #[serde(default)]
    pub archived: bool,
}

pub fn list_subjects(conn: &Connection) -> Result<Vec<SubjectRow>> {
    let mut stmt = conn.prepare("SELECT id, name, archived FROM subject ORDER BY name")?;
    let rows = stmt.query_map([], |r| {
        Ok(SubjectRow {
            id: r.get(0)?,
            name: r.get(1)?,
            archived: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn create_subject(conn: &Connection, s: &SubjectInput) -> Result<i64> {
    let name = clean_name(&s.name).ok_or(Error::Invalid("name"))?;
    conn.execute(
        "INSERT INTO subject (name, archived) VALUES (?1, ?2)",
        params![name, s.archived],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_subject(conn: &Connection, id: i64, s: &SubjectInput) -> Result<()> {
    let name = clean_name(&s.name).ok_or(Error::Invalid("name"))?;
    found(conn.execute(
        "UPDATE subject SET name = ?2, archived = ?3 WHERE id = ?1",
        params![id, name, s.archived],
    )?)
}

// ---- decks ----

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct DeckRow {
    pub id: i64,
    pub name: String,
    pub subject_id: i64,
    pub system_id: i64,
    pub direction_mode: String,
    pub answer_mode: String,
    pub archived: bool,
    pub children: Vec<i64>,
    /// Active cards.
    pub cards: i64,
}

#[derive(Deserialize)]
pub struct DeckInput {
    pub name: String,
    pub subject_id: i64,
    pub system_id: i64,
    pub direction_mode: String,
    #[serde(default = "default_answer_mode")]
    pub answer_mode: String,
    pub children: Vec<i64>,
    #[serde(default)]
    pub archived: bool,
}

fn default_answer_mode() -> String {
    "self".into()
}

fn check_deck(conn: &Connection, d: &DeckInput) -> Result<String> {
    let name = clean_name(&d.name).ok_or(Error::Invalid("name"))?;
    if !exists(
        conn,
        "SELECT EXISTS(SELECT 1 FROM subject WHERE id = ?1)",
        d.subject_id,
    )? {
        return Err(Error::Invalid("subject_id"));
    }
    if !exists(
        conn,
        "SELECT EXISTS(SELECT 1 FROM study_system WHERE id = ?1)",
        d.system_id,
    )? {
        return Err(Error::Invalid("system_id"));
    }
    if !["front_to_back", "back_to_front", "both"].contains(&d.direction_mode.as_str()) {
        return Err(Error::Invalid("direction_mode"));
    }
    if !["self", "timed"].contains(&d.answer_mode.as_str()) {
        return Err(Error::Invalid("answer_mode"));
    }
    // A timed deck asks the sum and wants the result, never the other way round.
    if d.answer_mode == "timed" && d.direction_mode != "front_to_back" {
        return Err(Error::Invalid("direction_mode"));
    }
    for &child in &d.children {
        if !exists(
            conn,
            "SELECT EXISTS(SELECT 1 FROM child WHERE id = ?1)",
            child,
        )? {
            return Err(Error::Invalid("children"));
        }
    }
    Ok(name)
}

fn set_children(conn: &Connection, deck_id: i64, children: &[i64]) -> Result<()> {
    // Unassigning only removes the link; the child's progress stays for a later return.
    conn.execute("DELETE FROM deck_child WHERE deck_id = ?1", [deck_id])?;
    for child in children {
        conn.execute(
            "INSERT OR IGNORE INTO deck_child (deck_id, child_id) VALUES (?1, ?2)",
            [deck_id, *child],
        )?;
    }
    Ok(())
}

pub fn list_decks(conn: &Connection) -> Result<Vec<DeckRow>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.name, d.subject_id, d.system_id, d.direction_mode, d.archived,
                (SELECT count(*) FROM card c WHERE c.deck_id = d.id AND c.active = 1),
                d.answer_mode
         FROM deck d ORDER BY d.name",
    )?;
    let mut children =
        conn.prepare("SELECT child_id FROM deck_child WHERE deck_id = ?1 ORDER BY child_id")?;
    let decks = stmt
        .query_map([], |r| {
            Ok(DeckRow {
                id: r.get(0)?,
                name: r.get(1)?,
                subject_id: r.get(2)?,
                system_id: r.get(3)?,
                direction_mode: r.get(4)?,
                answer_mode: r.get(7)?,
                archived: r.get(5)?,
                children: vec![],
                cards: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    decks
        .into_iter()
        .map(|mut d| {
            d.children = children
                .query_map([d.id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(d)
        })
        .collect()
}

pub fn create_deck(conn: &mut Connection, d: &DeckInput) -> Result<i64> {
    let name = check_deck(conn, d)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO deck (name, subject_id, system_id, direction_mode, archived, answer_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            name,
            d.subject_id,
            d.system_id,
            d.direction_mode,
            d.archived,
            d.answer_mode
        ],
    )?;
    let id = tx.last_insert_rowid();
    set_children(&tx, id, &d.children)?;
    tx.commit()?;
    Ok(id)
}

/// Edits a deck. Switching study system moves each progress record to the box at the
/// same position in the new system, or its last box; due dates are kept (spec §5.6).
pub fn update_deck(conn: &mut Connection, id: i64, d: &DeckInput) -> Result<()> {
    let name = check_deck(conn, d)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let old_system: Option<i64> = tx
        .query_row("SELECT system_id FROM deck WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(old_system) = old_system else {
        return Err(Error::NotFound);
    };
    tx.execute(
        "UPDATE deck SET name = ?2, subject_id = ?3, system_id = ?4, direction_mode = ?5,
                         archived = ?6, answer_mode = ?7
         WHERE id = ?1",
        params![
            id,
            name,
            d.subject_id,
            d.system_id,
            d.direction_mode,
            d.archived,
            d.answer_mode
        ],
    )?;
    if old_system != d.system_id {
        // engine::remap_positions with no kept boxes is exactly min(position, last).
        let last = store::load_boxes(&tx, d.system_id)?.len() - 1;
        tx.execute(
            "UPDATE progress SET box_position = min(box_position, ?2)
             WHERE card_id IN (SELECT id FROM card WHERE deck_id = ?1)",
            params![id, last],
        )?;
    }
    set_children(&tx, id, &d.children)?;
    tx.commit()?;
    Ok(())
}

// ---- cards ----

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct CardRow {
    pub id: i64,
    pub front_text: String,
    pub front_image: Option<i64>,
    pub back_text: String,
    pub back_image: Option<i64>,
    pub notes: String,
    pub active: bool,
}

#[derive(Clone, Deserialize)]
pub struct CardInput {
    pub front_text: String,
    #[serde(default)]
    pub front_image: Option<i64>,
    pub back_text: String,
    #[serde(default)]
    pub back_image: Option<i64>,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "yes")]
    pub active: bool,
}

fn yes() -> bool {
    true
}

/// Trimmed card texts, checked against spec §8: sides ≤ 500 characters, notes ≤ 1000,
/// and each side has text or an image (spec §4).
fn check_card(conn: &Connection, c: &CardInput) -> Result<(String, String, String)> {
    let side = |text: &str, image: Option<i64>, field| -> Result<String> {
        let text = text.trim();
        if text.chars().count() > 500 || (text.is_empty() && image.is_none()) {
            return Err(Error::Invalid(field));
        }
        Ok(text.to_string())
    };
    let front = side(&c.front_text, c.front_image, "front")?;
    let back = side(&c.back_text, c.back_image, "back")?;
    let notes = c.notes.trim();
    if notes.chars().count() > 1000 {
        return Err(Error::Invalid("notes"));
    }
    for (image, field) in [(c.front_image, "front_image"), (c.back_image, "back_image")] {
        if let Some(id) = image
            && !exists(conn, "SELECT EXISTS(SELECT 1 FROM image WHERE id = ?1)", id)?
        {
            return Err(Error::Invalid(field));
        }
    }
    Ok((front, back, notes.to_string()))
}

/// All cards of a deck, inactive ones too, in the order they were added.
pub fn list_cards(conn: &Connection, deck_id: i64) -> Result<Vec<CardRow>> {
    if !exists(
        conn,
        "SELECT EXISTS(SELECT 1 FROM deck WHERE id = ?1)",
        deck_id,
    )? {
        return Err(Error::NotFound);
    }
    let mut stmt = conn.prepare(
        "SELECT id, front_text, front_image_id, back_text, back_image_id, notes, active
         FROM card WHERE deck_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([deck_id], |r| {
        Ok(CardRow {
            id: r.get(0)?,
            front_text: r.get(1)?,
            front_image: r.get(2)?,
            back_text: r.get(3)?,
            back_image: r.get(4)?,
            notes: r.get(5)?,
            active: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Adds one or more cards (one for the card form, many for "Paste a list") in one
/// transaction: either all are saved or none.
pub fn create_cards(conn: &mut Connection, deck_id: i64, cards: &[CardInput]) -> Result<Vec<i64>> {
    if !exists(
        conn,
        "SELECT EXISTS(SELECT 1 FROM deck WHERE id = ?1)",
        deck_id,
    )? {
        return Err(Error::NotFound);
    }
    let checked = cards
        .iter()
        .map(|c| check_card(conn, c))
        .collect::<Result<Vec<_>>>()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut ids = Vec::with_capacity(cards.len());
    for (c, (front, back, notes)) in cards.iter().zip(checked) {
        tx.execute(
            "INSERT INTO card (deck_id, front_text, front_image_id, back_text, back_image_id,
                               notes, active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                deck_id,
                front,
                c.front_image,
                back,
                c.back_image,
                notes,
                c.active
            ],
        )?;
        ids.push(tx.last_insert_rowid());
    }
    tx.commit()?;
    Ok(ids)
}

/// Edits a card, or (in)activates it. Progress is kept either way.
pub fn update_card(conn: &Connection, id: i64, c: &CardInput) -> Result<()> {
    let (front, back, notes) = check_card(conn, c)?;
    found(conn.execute(
        "UPDATE card SET front_text = ?2, front_image_id = ?3, back_text = ?4,
                         back_image_id = ?5, notes = ?6, active = ?7
         WHERE id = ?1",
        params![
            id,
            front,
            c.front_image,
            back,
            c.back_image,
            notes,
            c.active
        ],
    )?)
}

// ---- images ----

/// The image type from the file's first bytes, never from what the browser claims.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Stores an uploaded image (JPEG, PNG, WebP or GIF, at most 5 MB) and returns its id.
pub fn add_image(conn: &Connection, bytes: &[u8]) -> Result<i64> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(Error::Invalid("image_size"));
    }
    let mime = sniff(bytes).ok_or(Error::Invalid("image_type"))?;
    conn.execute(
        "INSERT INTO image (mime, data) VALUES (?1, ?2)",
        params![mime, bytes],
    )?;
    Ok(conn.last_insert_rowid())
}

// ---- progress per deck ----

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ChildProgress {
    pub child_id: i64,
    pub name: String,
    pub due: i64,
    /// Active card directions per box, first box first.
    pub boxes: Vec<i64>,
}

/// Box distribution and due count per assigned, active child (spec §7.4).
pub fn deck_progress(
    conn: &Connection,
    deck_id: i64,
    today: NaiveDate,
) -> Result<Vec<ChildProgress>> {
    let system: Option<i64> = conn
        .query_row("SELECT system_id FROM deck WHERE id = ?1", [deck_id], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(system) = system else {
        return Err(Error::NotFound);
    };
    let box_count = store::load_boxes(conn, system)?.len();
    let mut children = conn.prepare(
        "SELECT c.id, c.name FROM deck_child dc JOIN child c ON c.id = dc.child_id
         WHERE dc.deck_id = ?1 AND c.archived = 0 ORDER BY c.id",
    )?;
    let children = children
        .query_map([deck_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut per_box = conn.prepare(&format!(
        "SELECT COALESCE(p.box_position, 0), count(*),
                sum(COALESCE(p.due_date, :today) <= :today)
         {VISIBLE} AND d.id = :deck GROUP BY 1"
    ))?;
    children
        .into_iter()
        .map(|(child_id, name)| {
            let mut boxes = vec![0; box_count];
            let mut due = 0;
            let rows = per_box.query_map(
                named_params! { ":child": child_id, ":today": today, ":deck": deck_id },
                |r| {
                    Ok((
                        r.get::<_, usize>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )?;
            for row in rows {
                let (pos, count, due_here) = row?;
                // A stale position past the last box counts as the last box (Plan 1).
                boxes[pos.min(box_count - 1)] += count;
                due += due_here;
            }
            Ok(ChildProgress {
                child_id,
                name,
                due,
                boxes,
            })
        })
        .collect()
}

/// Resets one assigned child, or every assigned child, on a deck (spec §5.7).
pub fn reset(
    conn: &Connection,
    deck_id: i64,
    child_id: Option<i64>,
    today: NaiveDate,
) -> Result<usize> {
    if !exists(
        conn,
        "SELECT EXISTS(SELECT 1 FROM deck WHERE id = ?1)",
        deck_id,
    )? {
        return Err(Error::NotFound);
    }
    if let Some(child) = child_id {
        let assigned: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM deck_child WHERE deck_id = ?1 AND child_id = ?2)",
            [deck_id, child],
            |r| r.get(0),
        )?;
        if !assigned {
            return Err(Error::Invalid("child_id"));
        }
    }
    Ok(store::reset_progress(conn, deck_id, child_id, today)?)
}

// ---- overview ----

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ChildOverview {
    pub child_id: i64,
    pub name: String,
    pub avatar: String,
    pub due: i64,
    /// Scheduling answers today, and how many of those were "known".
    pub reviewed: i64,
    pub known: i64,
    pub not_known: i64,
    /// "Known" answers today given within 2 seconds of the previous answer: clicking
    /// through without thinking (spec §7.1).
    pub rushed: i64,
    /// "Known" answers today on self-judged decks: what `rushed` is compared with.
    pub self_known: i64,
    /// Active card directions per box position, over all decks (spec §7.1).
    pub boxes: Vec<i64>,
}

/// The parent's start page (spec §7.1), per active child.
pub fn overview(conn: &Connection, today: NaiveDate) -> Result<Vec<ChildOverview>> {
    let mut children =
        conn.prepare("SELECT id, name, avatar FROM child WHERE archived = 0 ORDER BY id")?;
    let children = children
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut per_box = conn.prepare(&format!(
        "SELECT COALESCE(p.box_position, 0), count(*), sum(COALESCE(p.due_date, :today) <= :today)
         {VISIBLE} GROUP BY 1 ORDER BY 1"
    ))?;
    children
        .into_iter()
        .map(|(child_id, name, avatar)| {
            let mut boxes: Vec<i64> = vec![];
            let mut due = 0;
            let rows =
                per_box.query_map(named_params! { ":child": child_id, ":today": today }, |r| {
                    Ok((
                        r.get::<_, usize>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })?;
            for row in rows {
                let (pos, count, due_here) = row?;
                if boxes.len() <= pos {
                    boxes.resize(pos + 1, 0);
                }
                boxes[pos] += count;
                due += due_here;
            }
            let (reviewed, known, self_known): (i64, i64, i64) = conn.query_row(
                "SELECT count(*), COALESCE(sum(r.known), 0),
                        COALESCE(sum(r.known AND d.answer_mode = 'self'), 0)
                 FROM review_log r
                 JOIN card c ON c.id = r.card_id
                 JOIN deck d ON d.id = c.deck_id
                 WHERE r.child_id = ?1 AND r.local_date = ?2",
                params![child_id, today],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let rushed = conn.query_row(
                // The gap is to the previous answer of any deck; fast answers on timed decks
                // are the point of those decks, so only self-judged ones count.
                "SELECT count(*) FROM (
                   SELECT r.known, d.answer_mode,
                          (julianday(r.at) - lag(julianday(r.at)) OVER (ORDER BY julianday(r.at))) * 86400 AS gap
                   FROM review_log r
                   JOIN card c ON c.id = r.card_id
                   JOIN deck d ON d.id = c.deck_id
                   WHERE r.child_id = ?1 AND r.local_date = ?2)
                 WHERE known AND answer_mode = 'self' AND gap < 2",
                params![child_id, today],
                |r| r.get(0),
            )?;
            Ok(ChildOverview {
                child_id,
                name,
                avatar,
                due,
                reviewed,
                known,
                not_known: reviewed - known,
                rushed,
                self_known,
                boxes,
            })
        })
        .collect()
}

// ---- settings ----

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub name: String,
    pub timezone: String,
    pub default_system_id: i64,
}

pub fn settings(conn: &Connection) -> Result<Settings> {
    conn.query_row(
        "SELECT name, timezone, default_system_id FROM family",
        [],
        |r| {
            Ok(Settings {
                name: r.get(0)?,
                timezone: r.get(1)?,
                default_system_id: r.get(2)?,
            })
        },
    )
    .optional()?
    .ok_or(Error::NotFound)
}

/// Family name, timezone and default study system (spec §7.6, §7.7).
pub fn update_settings(conn: &Connection, s: &Settings) -> Result<()> {
    let name = clean_name(&s.name).ok_or(Error::Invalid("name"))?;
    s.timezone
        .parse::<Tz>()
        .map_err(|_| Error::Invalid("timezone"))?;
    if !exists(
        conn,
        "SELECT EXISTS(SELECT 1 FROM study_system WHERE id = ?1)",
        s.default_system_id,
    )? {
        return Err(Error::Invalid("default_system_id"));
    }
    found(conn.execute(
        "UPDATE family SET name = ?1, timezone = ?2, default_system_id = ?3",
        params![name, s.timezone, s.default_system_id],
    )?)
}

/// Stores a new PIN (spec §7.7). The current PIN is checked by the caller, under the same
/// lockout as the login (`Auth::attempt`).
pub fn set_pin(conn: &Connection, new: &str) -> Result<()> {
    if !auth::valid_pin(new) {
        return Err(Error::Invalid("pin"));
    }
    conn.execute("UPDATE family SET pin_hash = ?1", [auth::hash_pin(new)])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::store::{Answer, AnswerOutcome, due_items, record_answer};

    const TODAY: &str = "2026-10-18";

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn child(name: &str) -> ChildInput {
        ChildInput {
            name: name.into(),
            avatar: "#e57373".into(),
            archived: false,
            time_limit_ms: 3000,
        }
    }

    fn deck(children: Vec<i64>) -> DeckInput {
        DeckInput {
            name: "Woordjes H1".into(),
            subject_id: 1,
            system_id: 1,
            direction_mode: "front_to_back".into(),
            answer_mode: "self".into(),
            children,
            archived: false,
        }
    }

    fn card(front: &str, back: &str) -> CardInput {
        CardInput {
            front_text: front.into(),
            front_image: None,
            back_text: back.into(),
            back_image: None,
            notes: String::new(),
            active: true,
        }
    }

    /// Lotte (1) and Wout (2), subject Frans (1), deck 1 for Lotte with two cards.
    fn fixture() -> Connection {
        let mut conn = db::open_in_memory().unwrap();
        create_child(&conn, &child("Lotte")).unwrap();
        create_child(&conn, &child("Wout")).unwrap();
        create_subject(
            &conn,
            &SubjectInput {
                name: "Frans".into(),
                archived: false,
            },
        )
        .unwrap();
        create_deck(&mut conn, &deck(vec![1])).unwrap();
        create_cards(
            &mut conn,
            1,
            &[card("le chat", "de kat"), card("le chien", "de hond")],
        )
        .unwrap();
        conn
    }

    fn answer(conn: &mut Connection, id: &str, card_id: i64, known: bool) -> AnswerOutcome {
        let a = Answer {
            id: id.into(),
            child_id: 1,
            card_id,
            direction: 0,
            known,
        };
        let now = "2026-10-18T09:00:00Z".parse().unwrap();
        record_answer(conn, &a, now, d(TODAY)).unwrap()
    }

    fn invalid<T: std::fmt::Debug>(r: Result<T>) -> &'static str {
        match r {
            Err(Error::Invalid(f)) => f,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn children_are_created_renamed_and_archived() {
        let conn = fixture();
        update_child(
            &conn,
            2,
            &ChildInput {
                name: " Wouter ".into(),
                avatar: "#4fc3f7".into(),
                archived: true,
                time_limit_ms: 3000,
            },
        )
        .unwrap();
        assert_eq!(
            list_children(&conn).unwrap()[1],
            ChildRow {
                id: 2,
                name: "Wouter".into(),
                avatar: "#4fc3f7".into(),
                archived: true,
                time_limit_ms: 3000,
            }
        );
        assert_eq!(invalid(create_child(&conn, &child(" "))), "name");
        assert_eq!(
            invalid(create_child(
                &conn,
                &ChildInput {
                    avatar: "red".into(),
                    ..child("X")
                }
            )),
            "avatar"
        );
        assert!(matches!(
            update_child(&conn, 99, &child("X")),
            Err(Error::NotFound)
        ));
    }

    #[test]
    fn subjects_are_created_renamed_and_archived() {
        let conn = fixture();
        update_subject(
            &conn,
            1,
            &SubjectInput {
                name: "Frans 5de".into(),
                archived: true,
            },
        )
        .unwrap();
        assert_eq!(
            list_subjects(&conn).unwrap(),
            vec![SubjectRow {
                id: 1,
                name: "Frans 5de".into(),
                archived: true
            }]
        );
        assert_eq!(
            invalid(create_subject(
                &conn,
                &SubjectInput {
                    name: "".into(),
                    archived: false
                }
            )),
            "name"
        );
    }

    #[test]
    fn decks_list_with_children_and_active_card_count() {
        let mut conn = fixture();
        update_card(
            &conn,
            2,
            &CardInput {
                active: false,
                ..card("le chien", "de hond")
            },
        )
        .unwrap();
        update_deck(&mut conn, 1, &deck(vec![1, 2])).unwrap();
        let decks = list_decks(&conn).unwrap();
        assert_eq!(decks[0].children, vec![1, 2]);
        assert_eq!(decks[0].cards, 1);
    }

    #[test]
    fn deck_input_is_checked() {
        let mut conn = fixture();
        type Break = fn(&mut DeckInput);
        let cases: [(&str, Break); 7] = [
            ("name", |d| d.name = "".into()),
            ("subject_id", |d| d.subject_id = 9),
            ("system_id", |d| d.system_id = 9),
            ("direction_mode", |d| d.direction_mode = "sideways".into()),
            ("children", |d| d.children = vec![9]),
            ("answer_mode", |d| d.answer_mode = "fast".into()),
            // Timed decks only study front to back (tables spec §5).
            ("direction_mode", |d| {
                d.answer_mode = "timed".into();
                d.direction_mode = "both".into();
            }),
        ];
        for (field, break_it) in cases {
            let mut input = deck(vec![1]);
            break_it(&mut input);
            assert_eq!(invalid(create_deck(&mut conn, &input)), field);
        }
        assert!(matches!(
            update_deck(&mut conn, 99, &deck(vec![])),
            Err(Error::NotFound)
        ));
    }

    #[test]
    fn timed_decks_only_study_front_to_back() {
        let mut conn = fixture();
        let mut input = deck(vec![1]);
        input.answer_mode = "timed".into();
        update_deck(&mut conn, 1, &input).unwrap();
        assert_eq!(list_decks(&conn).unwrap()[0].answer_mode, "timed");
        input.direction_mode = "both".into();
        assert_eq!(invalid(update_deck(&mut conn, 1, &input)), "direction_mode");
    }

    #[test]
    fn children_get_a_time_limit_of_3_seconds_unless_set() {
        let conn = fixture();
        assert_eq!(list_children(&conn).unwrap()[0].time_limit_ms, 3000);
        let slower = ChildInput {
            time_limit_ms: 4500,
            ..child("Lotte")
        };
        update_child(&conn, 1, &slower).unwrap();
        assert_eq!(list_children(&conn).unwrap()[0].time_limit_ms, 4500);
    }

    #[test]
    fn time_limits_outside_1_to_30_seconds_or_off_the_half_second_are_refused() {
        let conn = fixture();
        for ms in [0, 500, 30500, 3200] {
            let c = ChildInput {
                time_limit_ms: ms,
                ..child("X")
            };
            assert_eq!(invalid(create_child(&conn, &c)), "time_limit_ms", "{ms}");
        }
    }

    #[test]
    fn unassigning_keeps_progress_for_a_later_return() {
        let mut conn = fixture();
        answer(&mut conn, "a1", 1, true); // box 2
        update_deck(&mut conn, 1, &deck(vec![])).unwrap();
        assert!(
            due_items(&conn, 1, None, d("2026-10-30"))
                .unwrap()
                .is_empty()
        );
        update_deck(&mut conn, 1, &deck(vec![1])).unwrap();
        let box_pos: i64 = conn
            .query_row("SELECT box_position FROM progress", [], |r| r.get(0))
            .unwrap();
        assert_eq!(box_pos, 1);
    }

    #[test]
    fn switching_system_moves_progress_to_same_or_last_box() {
        let mut conn = fixture();
        conn.execute_batch(
            "INSERT INTO study_system (id, name) VALUES (2, 'Twee bakjes');
             INSERT INTO box VALUES (2, 0, 'A', 1, 1, 0), (2, 1, 'B', 3, 1, 0);",
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO progress (child_id, card_id, direction, box_position, due_date)
             VALUES (1, 1, 0, 0, '2026-10-20'), (1, 2, 0, 3, '2026-11-01');",
        )
        .unwrap();
        update_deck(
            &mut conn,
            1,
            &DeckInput {
                system_id: 2,
                ..deck(vec![1])
            },
        )
        .unwrap();
        let rows: Vec<(i64, String)> = conn
            .prepare("SELECT box_position, due_date FROM progress ORDER BY card_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        // Same position kept; box 4 → last box; due dates untouched.
        assert_eq!(
            rows,
            vec![(0, "2026-10-20".into()), (1, "2026-11-01".into())]
        );
    }

    #[test]
    fn cards_are_trimmed_and_checked() {
        let mut conn = fixture();
        let ids = create_cards(&mut conn, 1, &[card("  la maison ", " het huis ")]).unwrap();
        let row = list_cards(&conn, 1)
            .unwrap()
            .into_iter()
            .find(|c| c.id == ids[0])
            .unwrap();
        assert_eq!(
            (row.front_text.as_str(), row.back_text.as_str()),
            ("la maison", "het huis")
        );

        assert_eq!(
            invalid(create_cards(&mut conn, 1, &[card(" ", "x")])),
            "front"
        );
        assert_eq!(
            invalid(create_cards(&mut conn, 1, &[card("x", &"é".repeat(501))])),
            "back"
        );
        assert_eq!(
            invalid(create_cards(
                &mut conn,
                1,
                &[CardInput {
                    notes: "n".repeat(1001),
                    ..card("x", "y")
                }]
            )),
            "notes"
        );
        assert_eq!(
            invalid(create_cards(
                &mut conn,
                1,
                &[CardInput {
                    front_image: Some(9),
                    ..card("x", "y")
                }]
            )),
            "front_image"
        );
        assert!(matches!(
            create_cards(&mut conn, 9, &[card("x", "y")]),
            Err(Error::NotFound)
        ));
        assert!(matches!(list_cards(&conn, 9), Err(Error::NotFound)));
    }

    #[test]
    fn a_side_may_be_only_an_image() {
        let mut conn = fixture();
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let image = add_image(&conn, &png).unwrap();
        create_cards(
            &mut conn,
            1,
            &[CardInput {
                front_text: "".into(),
                front_image: Some(image),
                ..card("", "de kat")
            }],
        )
        .unwrap();
    }

    #[test]
    fn pasted_cards_are_saved_all_or_nothing() {
        let mut conn = fixture();
        let batch = [card("un", "een"), card("deux", ""), card("trois", "drie")];
        assert_eq!(invalid(create_cards(&mut conn, 1, &batch)), "back");
        assert_eq!(list_cards(&conn, 1).unwrap().len(), 2);
        create_cards(&mut conn, 1, &[card("un", "een"), card("trois", "drie")]).unwrap();
        assert_eq!(list_cards(&conn, 1).unwrap().len(), 4);
    }

    #[test]
    fn inactivating_a_card_keeps_its_progress() {
        let mut conn = fixture();
        answer(&mut conn, "a1", 1, true);
        update_card(
            &conn,
            1,
            &CardInput {
                active: false,
                ..card("le chat", "de kat")
            },
        )
        .unwrap();
        update_card(&conn, 1, &card("le chat", "de kat")).unwrap();
        assert_eq!(answer(&mut conn, "a2", 1, true), AnswerOutcome::Practice); // still box 2, not due
        assert!(matches!(
            update_card(&conn, 99, &card("x", "y")),
            Err(Error::NotFound)
        ));
    }

    #[test]
    fn images_are_sniffed_not_trusted() {
        let conn = fixture();
        let cases: [(&[u8], &str); 4] = [
            (b"\x89PNG\r\n\x1a\n...", "image/png"),
            (&[0xFF, 0xD8, 0xFF, 0xE0], "image/jpeg"),
            (b"GIF89a...", "image/gif"),
            (b"RIFF\0\0\0\0WEBPVP8 ", "image/webp"),
        ];
        for (bytes, mime) in cases {
            let id = add_image(&conn, bytes).unwrap();
            let stored: String = conn
                .query_row("SELECT mime FROM image WHERE id = ?1", [id], |r| r.get(0))
                .unwrap();
            assert_eq!(stored, mime);
        }
        assert_eq!(
            invalid(add_image(&conn, b"<svg onload=alert(1)>")),
            "image_type"
        );
        assert_eq!(
            invalid(add_image(&conn, &vec![0xFF; MAX_IMAGE_BYTES + 1])),
            "image_size"
        );
    }

    #[test]
    fn deck_progress_counts_boxes_and_due_per_child() {
        let mut conn = fixture();
        update_deck(&mut conn, 1, &deck(vec![1, 2])).unwrap();
        answer(&mut conn, "a1", 1, true); // Lotte: card 1 → box 2, not due
        let p = deck_progress(&conn, 1, d(TODAY)).unwrap();
        assert_eq!(
            p,
            vec![
                ChildProgress {
                    child_id: 1,
                    name: "Lotte".into(),
                    due: 1,
                    boxes: vec![1, 1, 0, 0]
                },
                ChildProgress {
                    child_id: 2,
                    name: "Wout".into(),
                    due: 2,
                    boxes: vec![2, 0, 0, 0]
                },
            ]
        );
        assert!(matches!(
            deck_progress(&conn, 9, d(TODAY)),
            Err(Error::NotFound)
        ));
    }

    #[test]
    fn reset_checks_the_child_is_assigned() {
        let mut conn = fixture();
        answer(&mut conn, "a1", 1, true);
        assert_eq!(invalid(reset(&conn, 1, Some(2), d(TODAY))), "child_id");
        assert_eq!(reset(&conn, 1, Some(1), d(TODAY)).unwrap(), 1);
        assert_eq!(due_items(&conn, 1, None, d(TODAY)).unwrap().len(), 2);
        assert!(matches!(
            reset(&conn, 9, None, d(TODAY)),
            Err(Error::NotFound)
        ));
    }

    fn family(conn: &mut Connection) {
        crate::family::create(
            conn,
            &crate::family::Setup {
                family_name: "De Smet".into(),
                timezone: "Europe/Brussels".into(),
                pin: "2468".into(),
                child_name: "Lotte".into(),
                child_avatar: "#e57373".into(),
                sample_deck: true,
            },
        )
        .unwrap();
    }

    #[test]
    fn overview_counts_today_and_boxes_per_child() {
        let mut conn = db::open_in_memory().unwrap();
        family(&mut conn); // Lotte with the 10-card sample deck
        let a = |id: &str, card_id, known| Answer {
            id: id.into(),
            child_id: 1,
            card_id,
            direction: 0,
            known,
        };
        let now = "2026-10-18T09:00:00Z".parse().unwrap();
        record_answer(&mut conn, &a("a1", 1, true), now, d(TODAY)).unwrap();
        record_answer(&mut conn, &a("a2", 2, false), now, d(TODAY)).unwrap();
        assert_eq!(
            overview(&conn, d(TODAY)).unwrap(),
            vec![ChildOverview {
                child_id: 1,
                name: "Lotte".into(),
                avatar: "#e57373".into(),
                due: 8,
                reviewed: 2,
                known: 1,
                not_known: 1,
                rushed: 0,
                self_known: 1,
                boxes: vec![9, 1],
            }]
        );
    }

    #[test]
    fn overview_counts_known_answers_given_within_two_seconds() {
        let mut conn = db::open_in_memory().unwrap();
        family(&mut conn);
        let answers = [
            ("a1", 1, true, "09:00:00"),
            ("a2", 2, true, "09:00:01.5"), // fast
            ("a3", 3, true, "09:00:03"),   // fast
            ("a4", 4, false, "09:00:04"),  // fast, but honest "nog niet"
            ("a5", 5, true, "09:00:09"),   // took its time
        ];
        for (id, card_id, known, at) in answers {
            let a = Answer {
                id: id.into(),
                child_id: 1,
                card_id,
                direction: 0,
                known,
            };
            let now = format!("2026-10-18T{at}Z").parse().unwrap();
            record_answer(&mut conn, &a, now, d(TODAY)).unwrap();
        }
        assert_eq!(overview(&conn, d(TODAY)).unwrap()[0].rushed, 2);
    }

    #[test]
    fn fast_answers_on_timed_decks_are_not_rushing() {
        let mut conn = db::open_in_memory().unwrap();
        family(&mut conn); // Lotte with the 10-card sample deck (deck 1)
        conn.execute_batch(
            "INSERT INTO deck (id, subject_id, name, system_id, direction_mode, answer_mode)
             VALUES (2, 1, 'Tafels', 1, 'front_to_back', 'timed');
             INSERT INTO deck_child (deck_id, child_id) VALUES (2, 1);
             INSERT INTO card (id, deck_id, front_text, back_text, notes) VALUES
               (101, 2, '1 × 2', '2', ''), (102, 2, '2 × 2', '4', ''), (103, 2, '3 × 2', '6', ''),
               (104, 2, '4 × 2', '8', ''), (105, 2, '5 × 2', '10', '');",
        )
        .unwrap();
        // One second apart, alternating: sample card, sum, sample card, sum, …
        let cards = [1, 101, 2, 102, 3, 103, 104, 105];
        for (i, card_id) in cards.into_iter().enumerate() {
            let a = Answer {
                id: format!("a{i}"),
                child_id: 1,
                card_id,
                direction: 0,
                known: true,
            };
            let now = format!("2026-10-18T09:00:{i:02}Z").parse().unwrap();
            record_answer(&mut conn, &a, now, d(TODAY)).unwrap();
        }
        // Only the sample cards 2 and 3 count; card 1 was the first answer of the day.
        let o = &overview(&conn, d(TODAY)).unwrap()[0];
        assert_eq!(o.rushed, 2);
        // The "at least half" rule compares with self-judged known answers only.
        assert_eq!((o.known, o.self_known), (8, 3));
    }

    #[test]
    fn settings_are_read_and_checked() {
        let mut conn = db::open_in_memory().unwrap();
        assert!(matches!(settings(&conn), Err(Error::NotFound)));
        family(&mut conn);
        let s = Settings {
            name: "Familie De Smet".into(),
            timezone: "Europe/Amsterdam".into(),
            default_system_id: 1,
        };
        update_settings(&conn, &s).unwrap();
        assert_eq!(settings(&conn).unwrap(), s);
        for (field, bad) in [
            (
                "name",
                Settings {
                    name: "".into(),
                    ..settings(&conn).unwrap()
                },
            ),
            (
                "timezone",
                Settings {
                    timezone: "Mars/Olympus".into(),
                    ..settings(&conn).unwrap()
                },
            ),
            (
                "default_system_id",
                Settings {
                    default_system_id: 9,
                    ..settings(&conn).unwrap()
                },
            ),
        ] {
            assert_eq!(invalid(update_settings(&conn, &bad)), field);
        }
    }

    #[test]
    fn a_new_pin_is_checked_and_stored_hashed() {
        let mut conn = db::open_in_memory().unwrap();
        family(&mut conn);
        assert_eq!(invalid(set_pin(&conn, "12")), "pin");
        set_pin(&conn, "13579").unwrap();
        let hash: String = conn
            .query_row("SELECT pin_hash FROM family", [], |r| r.get(0))
            .unwrap();
        assert!(auth::verify_pin("13579", &hash) && !auth::verify_pin("2468", &hash));
    }
}
