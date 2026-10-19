//! Study systems (spec §5.6, §7.6). The preset is read-only; a custom system is saved as a
//! whole, and saving moves progress out of removed or reordered boxes in the same transaction.

use crate::engine::{self, BoxRule, SystemError};
use crate::family::clean_name;
use crate::manage::{Error, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

/// More boxes than this is a typo, not a study system.
const MAX_BOXES: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoxRow {
    pub name: String,
    pub interval_days: u32,
    pub success: usize,
    pub failure: usize,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SystemDetail {
    pub id: i64,
    pub name: String,
    pub preset: bool,
    pub repeat_missed: bool,
    pub boxes: Vec<BoxRow>,
    /// Decks using this system, archived ones included (they can come back).
    pub decks: i64,
    pub is_default: bool,
}

#[derive(Deserialize)]
pub struct BoxInput {
    pub name: String,
    pub interval_days: u32,
    pub success: usize,
    pub failure: usize,
    /// The position this box had before the edit; `None` for a new box.
    #[serde(default)]
    pub origin: Option<usize>,
}

#[derive(Deserialize)]
pub struct SystemInput {
    pub name: String,
    pub repeat_missed: bool,
    pub boxes: Vec<BoxInput>,
    /// On an edit: the boxes as the browser loaded them. If they no longer match, another
    /// tab saved in between and the origins can't be trusted, so the save is refused.
    #[serde(default)]
    pub previous: Option<Vec<BoxRow>>,
}

/// All systems, preset first, with their boxes.
pub fn list(conn: &Connection) -> Result<Vec<SystemDetail>> {
    let default: Option<i64> = conn
        .query_row("SELECT default_system_id FROM family", [], |r| r.get(0))
        .optional()?;
    let mut systems = conn.prepare(
        "SELECT s.id, s.name, s.is_preset, s.repeat_missed,
                (SELECT count(*) FROM deck d WHERE d.system_id = s.id)
         FROM study_system s ORDER BY s.is_preset DESC, s.name",
    )?;
    let mut boxes = conn.prepare(
        "SELECT name, interval_days, success_position, failure_position
         FROM box WHERE system_id = ?1 ORDER BY position",
    )?;
    let rows = systems
        .query_map([], |r| {
            Ok(SystemDetail {
                id: r.get(0)?,
                name: r.get(1)?,
                preset: r.get(2)?,
                repeat_missed: r.get(3)?,
                boxes: vec![],
                decks: r.get(4)?,
                is_default: false,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|mut s| {
            s.is_default = Some(s.id) == default;
            s.boxes = boxes
                .query_map([s.id], |r| {
                    Ok(BoxRow {
                        name: r.get(0)?,
                        interval_days: r.get(1)?,
                        success: r.get(2)?,
                        failure: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(s)
        })
        .collect()
}

/// Spec §5.6: saved as a whole, rejected without boxes or with a target outside the system.
fn check(s: &SystemInput) -> Result<(String, Vec<String>)> {
    let name = clean_name(&s.name).ok_or(Error::Invalid("name"))?;
    if s.boxes.len() > MAX_BOXES {
        return Err(Error::Invalid("boxes"));
    }
    let names = s
        .boxes
        .iter()
        .map(|b| clean_name(&b.name).ok_or(Error::Invalid("box_name")))
        .collect::<Result<Vec<_>>>()?;
    let rules: Vec<BoxRule> = s
        .boxes
        .iter()
        .map(|b| BoxRule {
            interval_days: b.interval_days,
            success: b.success,
            failure: b.failure,
        })
        .collect();
    engine::validate_system(&rules).map_err(|e| {
        Error::Invalid(match e {
            SystemError::NoBoxes => "boxes",
            SystemError::BadInterval { .. } => "interval_days",
            SystemError::BadTarget { .. } => "target",
        })
    })?;
    Ok((name, names))
}

fn insert_boxes(tx: &Transaction, id: i64, s: &SystemInput, names: &[String]) -> Result<()> {
    for (position, (b, name)) in s.boxes.iter().zip(names).enumerate() {
        tx.execute(
            "INSERT INTO box (system_id, position, name, interval_days, success_position,
                              failure_position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, position, name, b.interval_days, b.success, b.failure],
        )?;
    }
    Ok(())
}

/// Creates a custom system (also how "copy" works: the browser sends a copy's boxes).
pub fn create(conn: &mut Connection, s: &SystemInput) -> Result<i64> {
    let (name, names) = check(s)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO study_system (name, is_preset, repeat_missed) VALUES (?1, 0, ?2)",
        params![name, s.repeat_missed],
    )?;
    let id = tx.last_insert_rowid();
    insert_boxes(&tx, id, s, &names)?;
    tx.commit()?;
    Ok(id)
}

fn refuse_preset(tx: &Connection, id: i64) -> Result<()> {
    let preset: Option<bool> = tx
        .query_row(
            "SELECT is_preset FROM study_system WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    match preset {
        None => Err(Error::NotFound),
        Some(true) => Err(Error::Refused("preset", 0)),
        Some(false) => Ok(()),
    }
}

/// Replaces a custom system. Progress follows its box when boxes move; progress in a removed
/// box goes to the box now at that position, or the last box. Due dates are kept (§5.6).
pub fn update(conn: &mut Connection, id: i64, s: &SystemInput) -> Result<()> {
    let (name, names) = check(s)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    refuse_preset(&tx, id)?;
    let current: Vec<BoxRow> = tx
        .prepare(
            "SELECT name, interval_days, success_position, failure_position
             FROM box WHERE system_id = ?1 ORDER BY position",
        )?
        .query_map([id], |r| {
            Ok(BoxRow {
                name: r.get(0)?,
                interval_days: r.get(1)?,
                success: r.get(2)?,
                failure: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    if s.previous.as_ref().is_some_and(|p| *p != current) {
        return Err(Error::Invalid("origin"));
    }
    let old_len = current.len();
    let origins: Vec<Option<usize>> = s.boxes.iter().map(|b| b.origin).collect();
    let mut seen = vec![false; old_len];
    for &o in origins.iter().flatten() {
        if o >= old_len || seen[o] {
            return Err(Error::Invalid("origin"));
        }
        seen[o] = true;
    }
    let map = engine::remap_positions(old_len, &origins);
    // One statement for all positions, so a swap can't move a card twice. Positions past the
    // old last box (stale) go to the new last box. Only computed integers enter the SQL.
    let cases: String = map
        .iter()
        .enumerate()
        .map(|(old, new)| format!(" WHEN {old} THEN {new}"))
        .collect();
    tx.execute(
        &format!(
            "UPDATE progress SET box_position = CASE box_position{cases} ELSE ?2 END
             WHERE card_id IN (SELECT c.id FROM card c JOIN deck d ON d.id = c.deck_id
                               WHERE d.system_id = ?1)"
        ),
        params![id, s.boxes.len() - 1],
    )?;
    tx.execute(
        "UPDATE study_system SET name = ?2, repeat_missed = ?3 WHERE id = ?1",
        params![id, name, s.repeat_missed],
    )?;
    tx.execute("DELETE FROM box WHERE system_id = ?1", [id])?;
    insert_boxes(&tx, id, s, &names)?;
    tx.commit()?;
    Ok(())
}

/// Deletes an unused custom system. The preset, the family default and systems used by any
/// deck stay (§5.6); the refusal says why.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    refuse_preset(conn, id)?;
    let is_default: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM family WHERE default_system_id = ?1)",
        [id],
        |r| r.get(0),
    )?;
    if is_default {
        return Err(Error::Refused("default", 0));
    }
    let decks: i64 = conn.query_row(
        "SELECT count(*) FROM deck WHERE system_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if decks > 0 {
        return Err(Error::Refused("in_use", decks));
    }
    conn.execute("DELETE FROM study_system WHERE id = ?1", [id])?; // boxes cascade
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::family::{self, Setup};

    fn bx(
        name: &str,
        interval_days: u32,
        success: usize,
        failure: usize,
        origin: Option<usize>,
    ) -> BoxInput {
        BoxInput {
            name: name.into(),
            interval_days,
            success,
            failure,
            origin,
        }
    }

    /// Four boxes 1/3/7/30 days, known moves up, not known goes back to the first.
    fn four(origins: [Option<usize>; 4]) -> SystemInput {
        SystemInput {
            name: "Thuis".into(),
            repeat_missed: false,
            boxes: vec![
                bx("A", 1, 1, 0, origins[0]),
                bx("B", 3, 2, 0, origins[1]),
                bx("C", 7, 3, 0, origins[2]),
                bx("D", 30, 3, 0, origins[3]),
            ],
            previous: None,
        }
    }

    /// A family (default system = preset 1) with the sample deck, plus custom system 2 with
    /// a deck of its own whose single card has progress in each of its four boxes.
    fn fixture() -> Connection {
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
        let id = create(&mut conn, &four([None; 4])).unwrap();
        assert_eq!(id, 2);
        conn.execute_batch(
            "INSERT INTO child (id, name, avatar) VALUES (2, 'Wout', '#4fc3f7'), (3, 'An', '#ba68c8'), (4, 'Bo', '#4db6ac');
             INSERT INTO deck (id, subject_id, name, system_id, direction_mode) VALUES (2, 1, 'Thuis', 2, 'front_to_back');
             INSERT INTO card (id, deck_id, front_text, back_text) VALUES (100, 2, 'un', 'een');
             INSERT INTO progress (child_id, card_id, direction, box_position, due_date) VALUES
               (1, 100, 0, 0, '2026-10-20'), (2, 100, 0, 1, '2026-10-21'),
               (3, 100, 0, 2, '2026-10-22'), (4, 100, 0, 3, '2026-10-23');",
        )
        .unwrap();
        conn
    }

    /// (child, box, due) for the card on system 2, by child.
    fn progress(conn: &Connection) -> Vec<(i64, i64, String)> {
        conn.prepare("SELECT child_id, box_position, due_date FROM progress WHERE card_id = 100 ORDER BY child_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn invalid<T: std::fmt::Debug>(r: Result<T>) -> &'static str {
        match r {
            Err(Error::Invalid(f)) => f,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    fn refused<T: std::fmt::Debug>(r: Result<T>) -> (&'static str, i64) {
        match r {
            Err(Error::Refused(code, n)) => (code, n),
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn list_shows_preset_first_with_boxes_usage_and_default() {
        let conn = fixture();
        let systems = list(&conn).unwrap();
        assert_eq!(systems.len(), 2);
        let preset = &systems[0];
        assert!(preset.preset && preset.is_default);
        assert_eq!(preset.decks, 1);
        assert_eq!(
            preset.boxes[1],
            BoxRow {
                name: "Box 2".into(),
                interval_days: 2,
                success: 2,
                failure: 0
            }
        );
        assert_eq!(
            (
                systems[1].name.as_str(),
                systems[1].decks,
                systems[1].is_default
            ),
            ("Thuis", 1, false)
        );
    }

    #[test]
    fn input_is_checked_as_a_whole() {
        let mut conn = fixture();
        type Break = fn(&mut SystemInput);
        let cases: [(&str, Break); 6] = [
            ("name", |s| s.name = " ".into()),
            ("boxes", |s| s.boxes.clear()),
            ("boxes", |s| {
                s.boxes = (0..13).map(|_| bx("X", 1, 0, 0, None)).collect()
            }),
            ("box_name", |s| s.boxes[1].name = "".into()),
            ("interval_days", |s| s.boxes[2].interval_days = 0),
            ("target", |s| s.boxes[3].success = 4),
        ];
        for (field, break_it) in cases {
            let mut input = four([None; 4]);
            break_it(&mut input);
            assert_eq!(invalid(create(&mut conn, &input)), field);
            assert_eq!(invalid(update(&mut conn, 2, &input)), field);
        }
    }

    #[test]
    fn the_preset_is_read_only() {
        let mut conn = fixture();
        assert_eq!(
            refused(update(&mut conn, 1, &four([None; 4]))),
            ("preset", 0)
        );
        assert_eq!(refused(delete(&conn, 1)), ("preset", 0));
        assert!(matches!(
            update(&mut conn, 99, &four([None; 4])),
            Err(Error::NotFound)
        ));
    }

    #[test]
    fn removing_a_box_moves_its_progress_to_the_box_now_there() {
        let mut conn = fixture();
        // Remove B (old 1): the new boxes come from old [0, 2, 3].
        let input = SystemInput {
            name: "Thuis".into(),
            repeat_missed: false,
            boxes: vec![
                bx("A", 1, 1, 0, Some(0)),
                bx("C", 7, 2, 0, Some(2)),
                bx("D", 30, 2, 0, Some(3)),
            ],
            previous: None,
        };
        update(&mut conn, 2, &input).unwrap();
        assert_eq!(
            progress(&conn),
            vec![
                (1, 0, "2026-10-20".into()),
                (2, 1, "2026-10-21".into()), // B's card → box now at position 1 (C)
                (3, 1, "2026-10-22".into()), // C followed its box
                (4, 2, "2026-10-23".into()), // D followed its box
            ]
        );
    }

    #[test]
    fn reordering_moves_progress_with_its_box_without_collisions() {
        let mut conn = fixture();
        // Swap A and B: each box keeps the origin of the box it was.
        let mut input = four([Some(0), Some(1), Some(2), Some(3)]);
        input.boxes.swap(0, 1);
        update(&mut conn, 2, &input).unwrap();
        let boxes: Vec<i64> = progress(&conn).iter().map(|p| p.1).collect();
        assert_eq!(boxes, vec![1, 0, 2, 3]);
    }

    #[test]
    fn removing_the_last_boxes_moves_progress_to_the_new_last_box() {
        let mut conn = fixture();
        let input = SystemInput {
            name: "Kort".into(),
            repeat_missed: true,
            boxes: vec![bx("A", 1, 1, 0, Some(0)), bx("B", 5, 1, 0, Some(1))],
            previous: None,
        };
        update(&mut conn, 2, &input).unwrap();
        let boxes: Vec<i64> = progress(&conn).iter().map(|p| p.1).collect();
        assert_eq!(boxes, vec![0, 1, 1, 1]);
        let systems = list(&conn).unwrap();
        let s = systems.iter().find(|s| s.id == 2).unwrap();
        assert_eq!(
            (s.name.as_str(), s.repeat_missed, s.boxes.len()),
            ("Kort", true, 2)
        );
    }

    #[test]
    fn a_save_from_a_stale_page_is_refused() {
        let mut conn = fixture();
        let loaded = list(&conn).unwrap().remove(1).boxes; // both tabs load the same boxes
        // Tab B swaps A and B and saves.
        let mut swapped = four([Some(0), Some(1), Some(2), Some(3)]);
        swapped.boxes.swap(0, 1);
        swapped.previous = Some(loaded.clone());
        update(&mut conn, 2, &swapped).unwrap();
        // Tab A still has the old boxes: its origins look valid but no longer match.
        let mut stale = four([Some(0), Some(1), Some(2), Some(3)]);
        stale.previous = Some(loaded);
        assert_eq!(invalid(update(&mut conn, 2, &stale)), "origin");
        let boxes: Vec<i64> = progress(&conn).iter().map(|p| p.1).collect();
        assert_eq!(boxes, vec![1, 0, 2, 3]); // as tab B left it
    }

    #[test]
    fn origins_must_be_old_boxes_used_once() {
        let mut conn = fixture();
        assert_eq!(
            invalid(update(&mut conn, 2, &four([Some(0), Some(0), None, None]))),
            "origin"
        );
        assert_eq!(
            invalid(update(&mut conn, 2, &four([Some(4), None, None, None]))),
            "origin"
        );
    }

    #[test]
    fn delete_refuses_systems_in_use_or_default() {
        let mut conn = fixture();
        assert_eq!(refused(delete(&conn, 2)), ("in_use", 1));
        conn.execute_batch(
            "UPDATE deck SET system_id = 1 WHERE id = 2; UPDATE family SET default_system_id = 2",
        )
        .unwrap();
        assert_eq!(refused(delete(&conn, 2)), ("default", 0));
        let unused = create(&mut conn, &four([None; 4])).unwrap();
        delete(&conn, unused).unwrap();
        let boxes: i64 = conn
            .query_row(
                "SELECT count(*) FROM box WHERE system_id = ?1",
                [unused],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(boxes, 0);
    }
}
