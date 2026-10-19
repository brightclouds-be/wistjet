# Plan 4b: Study Systems, Overview & Settings — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Behind the PIN, a parent:
- starts on an overview of each child's day;
- copies the preset into custom study systems and edits them (boxes, intervals, "Wist ik" and "Wist ik niet" targets, repeat missed cards);
- deletes unused systems and picks the family default;
- changes family name, timezone and PIN.

**Architecture:**
- **Server.** `src/systems.rs` saves a study system as a whole. In the same transaction it moves progress out of removed or reordered boxes (`engine::remap_positions`, one `CASE` update), so swaps can't collide. `manage.rs` gains the overview, settings and PIN change, plus a `Refused(code, count)` error that the API answers as `409 { error, count }`. A PIN change ends every other parent session.
- **Browser.** `web/src/lib/systemEditor.ts` is a pure editor model whose links between boxes use stable keys, so reordering keeps them. It also applies the spec's reset rules when a box is removed. The parent area gains three tabs (Overzicht, Studiesystemen, Instellingen), and Overzicht becomes the start page.

**Tech Stack:** as before; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-08-family-flashcards-design.md` (§5.6, §7.1, §7.6, §7.7 except backup and restore, which is Plan 5). Roadmap: `docs/superpowers/plans/2026-10-11-wistjet-roadmap.md`.

## Decisions made while planning

- **One systems list.** `GET /api/parent/systems` now returns full details (boxes, decks using it, default flag). The 4a deck form reads `boxes.length` from it, so there aren't two lists.
- **Copying happens in the browser.** "Kopieer" opens the editor on a copy (`Kopie van …`, no origins), and saving it is an ordinary create.
- **At most 12 boxes per system.** That's a sanity limit; the spec sets none.
- **Box origins.** Each saved box says which old box it was (`origin`). The server uses that to keep progress with its box, and to send a removed box's progress to the box now at its position (or the last one). An origin used twice, or out of range, is refused.
- **A system can't be deleted** while it is the preset, the family default, or used by any deck (archived decks included, since they can come back). The refusal names the reason and the number of decks: "Dit studiesysteem wordt gebruikt door 3 decks."
- **The overview's box counts** are added up by box position over all of a child's decks ("Bakje 1…n"), because decks can use different systems. They are shown as numbers with a thin bar each: one series, so no legend, and the counts are the point.
- **A PIN change ends every other parent session**, so other devices have to unlock again with the new PIN.
- **Separators become labels.** The " · gearchiveerd" text separators from 4a are replaced by small `.tag` labels, because Svelte drops the leading space inside `<em> · …</em>`. The click-through caught it.

## Global Constraints

- Every new route lives under `/api/parent/` and needs a parent session.
- Spec §5.6: saving is rejected without boxes or with a target outside the system; intervals 1–365; due dates are never changed by an edit or a remap.
- All interface text is Dutch; error texts come from `messages.ts`.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `npm --prefix web run check` (0 errors, 0 warnings) and `npm --prefix web test` clean.

## Review Focus

1. **Two boxes swapped, then saved:** each child's cards follow their box and don't collide. Test `reordering_moves_progress_with_its_box_without_collisions` (Task 1).
2. **A box removed in the editor that other boxes pointed to:** the links reset to the spec's defaults and the parent sees what changed before saving. Tests in `systemEditor.test.ts` (Task 4).
3. **Deleting a system that is in use or the default:** refused, and the reason is explained. Tests `delete_refuses_systems_in_use_or_default` (Task 1), `a_system_in_use_says_by_how_many_decks` (Task 3).
4. **Changing the PIN while another device is unlocked:** that device is locked out. Tests `pin_change_keeps_only_the_current_session` (Task 2), `changing_the_pin_logs_out_other_devices` (Task 3).
5. **A study system edited by two tabs at once** (or saved from a stale page): impossible origins are refused instead of scrambling progress. Test `origins_must_be_old_boxes_used_once` (Task 1).

## File Structure

| File | Responsibility |
|---|---|
| `src/systems.rs` | List, create, update (with progress remap) and delete study systems |
| `src/manage.rs` | + `Error::Refused`, overview, settings, PIN change; − the light systems list |
| `src/auth.rs` | + `end_other_sessions` |
| `src/parent_api.rs` | + systems, overview, settings and PIN routes; 409 for refusals |
| `src/server.rs` | `session_token` becomes `pub(crate)` |
| `web/src/lib/systemEditor.ts` | Pure editor model (+ test) |
| `web/src/components/SystemEditor.svelte` | The box table editor |
| `web/src/routes/parent/{Overview,Systems,Settings}.svelte` | The three new tabs |

API added (all need a parent session):

| Method & path | Body → result |
|---|---|
| `GET /api/parent/systems` | now `[{ id, name, preset, repeat_missed, boxes: [{ name, interval_days, success, failure }], decks, is_default }]` |
| `POST /api/parent/systems` | `{ name, repeat_missed, boxes: [{ name, interval_days, success, failure, origin? }] }` → 201 `{ id }` |
| `PUT /api/parent/systems/{id}` | same → 204; 409 `{ error: "preset" }` |
| `DELETE /api/parent/systems/{id}` | → 204; 409 `{ error: "preset" \| "default" \| "in_use", count }` |
| `GET /api/parent/overview` | `[{ child_id, name, avatar, due, reviewed, known, not_known, boxes: [count] }]` |
| `GET/PUT /api/parent/settings` | `{ name, timezone, default_system_id }` |
| `POST /api/parent/pin` | `{ current, new }` → 204; 400 fields `current_pin`, `pin` |

Browser routes: `#/ouder` and `#/ouder/overzicht` (start), `#/ouder/systemen`, `#/ouder/instellingen`.

---

### Task 1: Study systems

**Files:**
- Create: `src/systems.rs`
- Modify: `src/manage.rs`, `src/parent_api.rs`, `src/lib.rs`

**Interfaces:**
- Consumes: `engine::{validate_system, remap_positions, SystemError, BoxRule}` (Plan 1); `family::clean_name`; `manage::{Error, Result}`
- Produces: `systems::{list, create, update, delete}`, `SystemDetail`, `BoxRow`, `SystemInput`, `BoxInput`; `manage::Error::Refused(&'static str, i64)`

- [ ] **Step 1: Write the failing tests**

In `src/lib.rs`, add after `pub mod study;`:
```rust
pub mod systems;
```
Create `src/systems.rs` with the doc comment and the test module:
```rust
//! Study systems (spec §5.6, §7.6). The preset is read-only; a custom system is saved as a
//! whole, and saving moves progress out of removed or reordered boxes in the same transaction.

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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib systems::`
Expected: compile errors (`cannot find function list`, `cannot find struct SystemInput`, `no variant Refused`).

- [ ] **Step 3: Implement**

In `src/manage.rs`, add to `enum Error` before `Db(rusqlite::Error),`:
```rust
    /// Not allowed in this state, with a count where it helps explain why
    /// (e.g. `("in_use", 3)`: a study system used by 3 decks).
    Refused(&'static str, i64),
```
Also in `src/manage.rs`, delete the whole `// ---- study systems (read-only here; the editor is Plan 4b) ----` section (`SystemRow` and `list_systems`) and its test `systems_list_shows_the_preset`. `systems::list` replaces them.

In `src/parent_api.rs`:
- in `list_systems`, replace `manage::list_systems(` with `crate::systems::list(`;
- in `fail`, add an arm before `Error::Db(e) => internal(e),`:
```rust
        Error::Refused(code, count) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": code, "count": count })),
        )
            .into_response(),
```

In `src/systems.rs`, insert between the doc comment and `#[cfg(test)]`:
```rust
use crate::engine::{self, BoxRule, SystemError};
use crate::family::clean_name;
use crate::manage::{Error, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

/// More boxes than this is a typo, not a study system.
const MAX_BOXES: usize = 12;

#[derive(Debug, PartialEq, Eq, Serialize)]
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
    let old_len: usize =
        tx.query_row("SELECT count(*) FROM box WHERE system_id = ?1", [id], |r| {
            r.get(0)
        })?;
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: 8 new tests pass (and `manage` loses one); fmt and clippy clean. `cargo test --test api` still passes.

- [ ] **Step 5: Commit**

```bash
git add src/systems.rs src/manage.rs src/parent_api.rs src/lib.rs
git commit -m "feat: custom study systems with progress remapping"
```

---

### Task 2: Overview, settings and PIN change

**Files:**
- Modify: `src/manage.rs`, `src/auth.rs`

**Interfaces:**
- Consumes: `store::VISIBLE`, `auth::{verify_pin, valid_pin, hash_pin}`
- Produces: `manage::{overview, settings, update_settings, change_pin}`, `ChildOverview`, `Settings` (`Serialize + Deserialize`); `Auth::end_other_sessions(&mut self, keep: &str)`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module of `src/manage.rs`:
```rust
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
                boxes: vec![9, 1],
            }]
        );
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
    fn pin_change_needs_the_current_pin() {
        let mut conn = db::open_in_memory().unwrap();
        family(&mut conn);
        assert_eq!(invalid(change_pin(&conn, "1111", "13579")), "current_pin");
        assert_eq!(invalid(change_pin(&conn, "2468", "12")), "pin");
        change_pin(&conn, "2468", "13579").unwrap();
        let hash: String = conn
            .query_row("SELECT pin_hash FROM family", [], |r| r.get(0))
            .unwrap();
        assert!(auth::verify_pin("13579", &hash) && !auth::verify_pin("2468", &hash));
    }
```
In `src/auth.rs`, add to the `tests` module before `fn logout_ends_the_session`:
```rust
    #[test]
    fn pin_change_keeps_only_the_current_session() {
        let mut auth = Auth::default();
        let t0 = Instant::now();
        let mine = token(auth.login(t0, || true));
        let other = token(auth.login(t0, || true));
        auth.end_other_sessions(&mine);
        assert!(auth.touch(&mine, t0));
        assert!(!auth.touch(&other, t0));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib`
Expected: compile errors (`cannot find function overview`, `no method end_other_sessions`).

- [ ] **Step 3: Implement**

In `src/manage.rs`, add `use crate::auth;` and `use chrono_tz::Tz;` to the imports, then insert before `#[cfg(test)]`:
```rust
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
            let (reviewed, known): (i64, i64) = conn.query_row(
                "SELECT count(*), COALESCE(sum(known), 0) FROM review_log
                 WHERE child_id = ?1 AND local_date = ?2",
                params![child_id, today],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            Ok(ChildOverview {
                child_id,
                name,
                avatar,
                due,
                reviewed,
                known,
                not_known: reviewed - known,
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

/// Changes the PIN; the current PIN must be given (spec §7.7).
pub fn change_pin(conn: &Connection, current: &str, new: &str) -> Result<()> {
    let hash: String = conn
        .query_row("SELECT pin_hash FROM family", [], |r| r.get(0))
        .optional()?
        .ok_or(Error::NotFound)?;
    if !auth::verify_pin(current, &hash) {
        return Err(Error::Invalid("current_pin"));
    }
    if !auth::valid_pin(new) {
        return Err(Error::Invalid("pin"));
    }
    conn.execute("UPDATE family SET pin_hash = ?1", [auth::hash_pin(new)])?;
    Ok(())
}
```
In `src/auth.rs`, add to `impl Auth` after `logout`:
```rust
    /// Ends every session but `keep` (after a PIN change, other devices log in again).
    pub fn end_other_sessions(&mut self, keep: &str) {
        self.sessions.retain(|token, _| token == keep);
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib && cargo clippy --all-targets -- -D warnings`
Expected: 4 new tests pass; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/manage.rs src/auth.rs
git commit -m "feat: parent overview, family settings and PIN change"
```

---

### Task 3: API routes

**Files:**
- Modify: `src/parent_api.rs`, `src/server.rs`, `tests/api.rs`

**Interfaces:**
- Consumes: Tasks 1 and 2
- Produces: the API table above

- [ ] **Step 1: Write the failing tests**

Append to `tests/api.rs`:
```rust
// ---- study systems, overview and settings (Plan 4b) ----

fn custom_system() -> Value {
    json!({
        "name": "Thuis",
        "repeat_missed": false,
        "boxes": [
            { "name": "Nieuw", "interval_days": 1, "success": 1, "failure": 0 },
            { "name": "Gekend", "interval_days": 5, "success": 1, "failure": 0 }
        ]
    })
}

#[tokio::test]
async fn system_overview_and_settings_routes_need_a_parent_session() {
    let app = app();
    set_up(&app).await;
    let routes = [
        ("POST", "/api/parent/systems"),
        ("PUT", "/api/parent/systems/1"),
        ("DELETE", "/api/parent/systems/1"),
        ("GET", "/api/parent/overview"),
        ("GET", "/api/parent/settings"),
        ("PUT", "/api/parent/settings"),
        ("POST", "/api/parent/pin"),
    ];
    for (method, path) in routes {
        let r = call(&app, method, path, None, Some(json!({}))).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn parent_creates_edits_and_deletes_a_study_system() {
    let (app, c) = parent().await;
    let r = call(&app, "GET", "/api/parent/systems", Some(&c), None).await;
    let preset = r.json()[0].clone();
    assert_eq!(
        (
            preset["preset"].clone(),
            preset["is_default"].clone(),
            preset["decks"].clone()
        ),
        (json!(true), json!(true), json!(1))
    );
    assert_eq!(preset["boxes"].as_array().unwrap().len(), 4);

    let r = call(
        &app,
        "POST",
        "/api/parent/systems",
        Some(&c),
        Some(custom_system()),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let id = r.json()["id"].as_i64().unwrap();
    let path = format!("/api/parent/systems/{id}");
    let mut edited = custom_system();
    edited["boxes"][1]["interval_days"] = json!(10);
    let r = call(&app, "PUT", &path, Some(&c), Some(edited)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    let r = call(
        &app,
        "PUT",
        "/api/parent/systems/1",
        Some(&c),
        Some(custom_system()),
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (
            StatusCode::CONFLICT,
            json!({ "error": "preset", "count": 0 })
        )
    );
    let r = call(&app, "DELETE", "/api/parent/systems/1", Some(&c), None).await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    let r = call(&app, "DELETE", &path, Some(&c), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/systems", Some(&c), None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_system_in_use_says_by_how_many_decks() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/systems",
        Some(&c),
        Some(custom_system()),
    )
    .await;
    let id = r.json()["id"].as_i64().unwrap();
    let deck = json!({ "name": "Voorbeeld", "subject_id": 1, "system_id": id, "direction_mode": "front_to_back", "children": [1] });
    let r = call(&app, "PUT", "/api/parent/decks/1", Some(&c), Some(deck)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(
        &app,
        "DELETE",
        &format!("/api/parent/systems/{id}"),
        Some(&c),
        None,
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (
            StatusCode::CONFLICT,
            json!({ "error": "in_use", "count": 1 })
        )
    );
}

#[tokio::test]
async fn overview_shows_today_per_child() {
    let (app, c) = parent().await;
    call(
        &app,
        "POST",
        "/api/answers",
        None,
        Some(answer_body("a1", 1)),
    )
    .await;
    let r = call(&app, "GET", "/api/parent/overview", Some(&c), None).await;
    assert_eq!(
        r.json(),
        json!([{
            "child_id": 1, "name": "Lotte", "avatar": "#e57373",
            "due": 9, "reviewed": 1, "known": 1, "not_known": 0, "boxes": [9, 1]
        }])
    );
}

#[tokio::test]
async fn settings_change_name_timezone_and_default_system() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/systems",
        Some(&c),
        Some(custom_system()),
    )
    .await;
    let id = r.json()["id"].clone();
    let body = json!({ "name": "Familie De Smet", "timezone": "Europe/Amsterdam", "default_system_id": id });
    let r = call(
        &app,
        "PUT",
        "/api/parent/settings",
        Some(&c),
        Some(body.clone()),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/settings", Some(&c), None).await;
    assert_eq!(r.json(), body);
    let r = call(&app, "GET", "/api/parent/systems", Some(&c), None).await;
    let custom = r
        .json()
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(custom["is_default"], json!(true));

    let mut bad = body;
    bad["timezone"] = json!("Mars/Olympus");
    let r = call(&app, "PUT", "/api/parent/settings", Some(&c), Some(bad)).await;
    assert_eq!(r.json(), json!({ "error": "invalid", "field": "timezone" }));
}

#[tokio::test]
async fn changing_the_pin_logs_out_other_devices() {
    let (app, mine) = parent().await;
    let other = log_in(&app).await;
    let wrong = json!({ "current": "1111", "new": "13579" });
    let r = call(&app, "POST", "/api/parent/pin", Some(&mine), Some(wrong)).await;
    assert_eq!(
        r.json(),
        json!({ "error": "invalid", "field": "current_pin" })
    );

    let r = call(
        &app,
        "POST",
        "/api/parent/pin",
        Some(&mine),
        Some(json!({ "current": "2468", "new": "13579" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/session", Some(&mine), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/session", Some(&other), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = call(
        &app,
        "POST",
        "/api/parent/login",
        None,
        Some(json!({ "pin": "13579" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix web run build && cargo test --test api`
Expected: the new tests fail (404 for the routes that don't exist yet).

- [ ] **Step 3: Implement**

In `src/server.rs`, change `fn session_token(` to `pub(crate) fn session_token(`.

`src/parent_api.rs` becomes:
```rust
//! Parent routes for content management (spec §7.2–7.4). `server::router` mounts these
//! inside the PIN-guarded `/api/parent` router, so every one needs a parent session.

use crate::manage::{self, CardInput, ChildInput, DeckInput, Error, Settings, SubjectInput};
use crate::server::{Shared, error, internal, session_token, today};
use crate::systems::{self, SystemInput};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/children", get(list_children).post(create_child))
        .route("/children/{id}", put(update_child))
        .route("/subjects", get(list_subjects).post(create_subject))
        .route("/subjects/{id}", put(update_subject))
        .route("/systems", get(list_systems).post(create_system))
        .route("/systems/{id}", put(update_system).delete(delete_system))
        .route("/overview", get(overview))
        .route("/settings", get(settings).put(update_settings))
        .route("/pin", post(change_pin))
        .route("/decks", get(list_decks).post(create_deck))
        .route("/decks/{id}", put(update_deck))
        .route("/decks/{id}/cards", get(list_cards).post(create_cards))
        .route("/decks/{id}/progress", get(deck_progress))
        .route("/decks/{id}/reset", post(reset))
        .route("/cards/{id}", put(update_card))
        // Room for a 5 MB image; manage::add_image rejects anything larger with a clear error.
        .route(
            "/images",
            post(add_image).layer(DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
}

fn fail(e: Error) -> Response {
    match e {
        Error::Invalid(field) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid", "field": field })),
        )
            .into_response(),
        Error::NotFound => error(StatusCode::NOT_FOUND, "not_found"),
        Error::Refused(code, count) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": code, "count": count })),
        )
            .into_response(),
        Error::Db(e) => internal(e),
    }
}

fn ok<T: Serialize>(r: manage::Result<T>) -> Response {
    r.map_or_else(fail, |v| Json(v).into_response())
}

fn created(r: manage::Result<i64>) -> Response {
    r.map_or_else(fail, |id| {
        (StatusCode::CREATED, Json(json!({ "id": id }))).into_response()
    })
}

fn done(r: manage::Result<()>) -> Response {
    r.map_or_else(fail, |()| StatusCode::NO_CONTENT.into_response())
}

async fn list_children(State(s): State<Shared>) -> Response {
    ok(manage::list_children(&s.db.lock().unwrap()))
}

async fn create_child(State(s): State<Shared>, Json(c): Json<ChildInput>) -> Response {
    created(manage::create_child(&s.db.lock().unwrap(), &c))
}

async fn update_child(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(c): Json<ChildInput>,
) -> Response {
    done(manage::update_child(&s.db.lock().unwrap(), id, &c))
}

async fn list_subjects(State(s): State<Shared>) -> Response {
    ok(manage::list_subjects(&s.db.lock().unwrap()))
}

async fn create_subject(State(s): State<Shared>, Json(b): Json<SubjectInput>) -> Response {
    created(manage::create_subject(&s.db.lock().unwrap(), &b))
}

async fn update_subject(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(b): Json<SubjectInput>,
) -> Response {
    done(manage::update_subject(&s.db.lock().unwrap(), id, &b))
}

async fn list_systems(State(s): State<Shared>) -> Response {
    ok(systems::list(&s.db.lock().unwrap()))
}

async fn create_system(State(s): State<Shared>, Json(b): Json<SystemInput>) -> Response {
    created(systems::create(&mut s.db.lock().unwrap(), &b))
}

async fn update_system(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(b): Json<SystemInput>,
) -> Response {
    done(systems::update(&mut s.db.lock().unwrap(), id, &b))
}

async fn delete_system(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    done(systems::delete(&s.db.lock().unwrap(), id))
}

async fn overview(State(s): State<Shared>) -> Response {
    let conn = s.db.lock().unwrap();
    match today(&conn) {
        Ok(Some(t)) => ok(manage::overview(&conn, t)),
        Ok(None) => error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => internal(e),
    }
}

async fn settings(State(s): State<Shared>) -> Response {
    ok(manage::settings(&s.db.lock().unwrap()))
}

async fn update_settings(State(s): State<Shared>, Json(b): Json<Settings>) -> Response {
    done(manage::update_settings(&s.db.lock().unwrap(), &b))
}

#[derive(Deserialize)]
struct PinChange {
    current: String,
    new: String,
}

/// Changes the PIN and ends every other parent session, so other devices must use the new one.
async fn change_pin(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(b): Json<PinChange>,
) -> Response {
    let result = manage::change_pin(&s.db.lock().unwrap(), &b.current, &b.new);
    if result.is_ok()
        && let Some(token) = session_token(&headers)
    {
        s.auth.lock().unwrap().end_other_sessions(token);
    }
    done(result)
}

async fn list_decks(State(s): State<Shared>) -> Response {
    ok(manage::list_decks(&s.db.lock().unwrap()))
}

async fn create_deck(State(s): State<Shared>, Json(d): Json<DeckInput>) -> Response {
    created(manage::create_deck(&mut s.db.lock().unwrap(), &d))
}

async fn update_deck(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(d): Json<DeckInput>,
) -> Response {
    done(manage::update_deck(&mut s.db.lock().unwrap(), id, &d))
}

async fn list_cards(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    ok(manage::list_cards(&s.db.lock().unwrap(), id))
}

/// One card from the card form, or many from "Paste a list"; all or nothing.
async fn create_cards(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(cards): Json<Vec<CardInput>>,
) -> Response {
    match manage::create_cards(&mut s.db.lock().unwrap(), id, &cards) {
        Ok(ids) => (StatusCode::CREATED, Json(json!({ "ids": ids }))).into_response(),
        Err(e) => fail(e),
    }
}

async fn update_card(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Json(c): Json<CardInput>,
) -> Response {
    done(manage::update_card(&s.db.lock().unwrap(), id, &c))
}

/// Takes the raw image as the body. Only `image/*` is accepted: an HTML form can only send
/// `text/plain`, `multipart/form-data` or `application/x-www-form-urlencoded`, so a page from
/// another site (or another app on this host, which counts as same-site and gets the parent's
/// cookie) can't upload without a CORS preflight, which this server never answers.
async fn add_image(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let is_image = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.starts_with("image/"));
    if !is_image {
        return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "image_type");
    }
    created(manage::add_image(&s.db.lock().unwrap(), &body))
}

async fn deck_progress(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    let conn = s.db.lock().unwrap();
    match today(&conn) {
        Ok(Some(t)) => ok(manage::deck_progress(&conn, id, t)),
        Ok(None) => error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct ResetBody {
    /// `None` resets every assigned child.
    child_id: Option<i64>,
}

async fn reset(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<ResetBody>) -> Response {
    let conn = s.db.lock().unwrap();
    match today(&conn) {
        Ok(Some(t)) => done(manage::reset(&conn, id, b.child_id, t).map(|_| ())),
        Ok(None) => error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => internal(e),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all pass (6 new in `api`); fmt and clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/parent_api.rs src/server.rs tests/api.rs
git commit -m "feat: API for study systems, overview, settings and PIN"
```

---

### Task 4: Editor model and shared web changes

**Files:**
- Create: `web/src/lib/systemEditor.ts`, `web/src/lib/systemEditor.test.ts`
- Modify: `web/src/lib/types.ts`, `web/src/lib/messages.ts`, `web/src/lib/api.ts`, `web/src/components/DeckForm.svelte`, `web/src/routes/parent/Decks.svelte`, `web/src/routes/parent/Deck.svelte`

**Interfaces:**
- Produces: `EditBox`, `Editor`, `fromSystem(s, copy?)`, `toInput(e)`, `addBox(e)`, `removeBox(e, key)`, `moveBox(e, key, -1 | 1)`; types `BoxRow`, `SystemDetail`, `BoxInput`, `SystemInput`, `ChildOverview`, `Settings`; `problem()` explains 409 refusals

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/systemEditor.test.ts`:
```ts
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { addBox, fromSystem, moveBox, removeBox, toInput } from './systemEditor.ts'
import type { SystemDetail } from './types.ts'

const SCHOOL: SystemDetail = {
  id: 1,
  name: 'School 4-box',
  preset: true,
  repeat_missed: true,
  decks: 1,
  is_default: true,
  boxes: [
    { name: 'Box 1', interval_days: 1, success: 1, failure: 0 },
    { name: 'Box 2', interval_days: 2, success: 2, failure: 0 },
    { name: 'Box 3', interval_days: 7, success: 3, failure: 0 },
    { name: 'Box 4', interval_days: 14, success: 3, failure: 0 },
  ],
}

test('a loaded system saves back unchanged', () => {
  const input = toInput(fromSystem(SCHOOL))
  assert.deepEqual(
    input.boxes.map((b) => [b.success, b.failure, b.origin]),
    [[1, 0, 0], [2, 0, 1], [3, 0, 2], [3, 0, 3]],
  )
})

test('a copy has no origins and a new name', () => {
  const e = fromSystem(SCHOOL, true)
  assert.equal(e.name, 'Kopie van School 4-box')
  assert.ok(toInput(e).boxes.every((b) => b.origin === null))
})

test('removing a box resets links to it and says so', () => {
  let e = fromSystem(SCHOOL)
  e = removeBox(e, e.boxes[2].key) // Box 3; Box 2 → Box 3 on success
  const input = toInput(e)
  assert.deepEqual(input.boxes.map((b) => b.name), ['Box 1', 'Box 2', 'Box 4'])
  // Box 2's success pointed to Box 3: now its next box (Box 4).
  assert.deepEqual(input.boxes.map((b) => [b.success, b.failure, b.origin]), [[1, 0, 0], [2, 0, 1], [2, 0, 3]])
  assert.deepEqual(e.notices, ['"Box 2" bij Wist ik gaat nu naar "Box 4".'])
})

test('a removed success target of the last box becomes the box itself', () => {
  let e = fromSystem(SCHOOL)
  const last = e.boxes[3]
  e = { ...e, boxes: e.boxes.map((b) => (b.key === last.key ? { ...b, success: e.boxes[2].key } : b)) }
  e = removeBox(e, e.boxes[2].key)
  assert.equal(toInput(e).boxes[2].success, 2)
})

test('a removed failure target becomes the first box', () => {
  let e = fromSystem(SCHOOL)
  e = { ...e, boxes: e.boxes.map((b, i) => (i === 3 ? { ...b, failure: e.boxes[1].key } : b)) }
  e = removeBox(e, e.boxes[1].key)
  assert.equal(toInput(e).boxes[2].failure, 0)
  assert.ok(e.notices.some((n) => n.includes('Wist ik niet')))
})

test('moving boxes keeps their links', () => {
  let e = fromSystem(SCHOOL)
  e = moveBox(e, e.boxes[1].key, -1) // Box 2 to the top
  const input = toInput(e)
  assert.deepEqual(input.boxes.map((b) => b.name), ['Box 2', 'Box 1', 'Box 3', 'Box 4'])
  // Box 2 still goes to Box 3 (now position 2) and back to Box 1 (now position 1).
  assert.deepEqual([input.boxes[0].success, input.boxes[0].failure], [2, 1])
  assert.deepEqual(input.boxes.map((b) => b.origin), [1, 0, 2, 3])
})

test('moving past either end changes nothing', () => {
  const e = fromSystem(SCHOOL)
  assert.equal(moveBox(e, e.boxes[0].key, -1), e)
  assert.equal(moveBox(e, e.boxes[3].key, 1), e)
})

test('a new box goes last, keeps cards that are known, and sends missed ones to the start', () => {
  let e = addBox(fromSystem(SCHOOL))
  const input = toInput(e)
  assert.equal(input.boxes.length, 5)
  assert.deepEqual(input.boxes[4], { name: 'Bakje 5', interval_days: 28, success: 4, failure: 0, origin: null })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npm --prefix web test`
Expected: FAIL, `Cannot find module …/systemEditor.ts`.

- [ ] **Step 3: Implement**

Create `web/src/lib/systemEditor.ts`:
```ts
import type { SystemDetail, SystemInput } from './types'

/** A box in the editor. `success` and `failure` point at other boxes' `key`s, so links
 *  survive reordering; positions are only worked out when saving. `origin` is the box's
 *  position when loaded (`null` for new boxes and copies), which the server uses to move
 *  progress along (spec §5.6). */
export type EditBox = {
  key: number
  origin: number | null
  name: string
  interval_days: number
  success: number
  failure: number
}

export type Editor = {
  name: string
  repeat_missed: boolean
  boxes: EditBox[]
  nextKey: number
  /** What a box removal changed, shown before saving (spec §7.6). */
  notices: string[]
}

export function fromSystem(s: SystemDetail, copy = false): Editor {
  return {
    name: copy ? `Kopie van ${s.name}`.slice(0, 50) : s.name,
    repeat_missed: s.repeat_missed,
    boxes: s.boxes.map((b, i) => ({ key: i, origin: copy ? null : i, ...b })),
    nextKey: s.boxes.length,
    notices: [],
  }
}

export function toInput(e: Editor): SystemInput {
  const pos = (key: number) => e.boxes.findIndex((b) => b.key === key)
  return {
    name: e.name,
    repeat_missed: e.repeat_missed,
    boxes: e.boxes.map((b) => ({
      name: b.name,
      interval_days: b.interval_days,
      success: pos(b.success),
      failure: pos(b.failure),
      origin: b.origin,
    })),
  }
}

/** A new last box: double the last interval, known stays here, missed goes to the start. */
export function addBox(e: Editor): Editor {
  const last = e.boxes[e.boxes.length - 1]
  const box: EditBox = {
    key: e.nextKey,
    origin: null,
    name: `Bakje ${e.boxes.length + 1}`,
    interval_days: Math.min(365, (last?.interval_days ?? 1) * 2),
    success: e.nextKey,
    failure: e.boxes[0]?.key ?? e.nextKey,
  }
  return { ...e, boxes: [...e.boxes, box], nextKey: e.nextKey + 1 }
}

/** Removes a box (never the last one left). Links to it reset to the defaults of spec §7.6:
 *  "Wist ik" → the next box, or the box itself when it is last; "Wist ik niet" → the first box. */
export function removeBox(e: Editor, key: number): Editor {
  if (e.boxes.length === 1 || !e.boxes.some((b) => b.key === key)) return e
  const boxes = e.boxes.filter((b) => b.key !== key)
  const notices = [...e.notices]
  const fixed = boxes.map((b, i) => {
    let { success, failure } = b
    if (success === key) {
      const next = boxes[i + 1] ?? b
      success = next.key
      notices.push(`"${b.name}" bij Wist ik gaat nu naar "${next.name}".`)
    }
    if (failure === key) {
      failure = boxes[0].key
      notices.push(`"${b.name}" bij Wist ik niet gaat nu naar "${boxes[0].name}".`)
    }
    return { ...b, success, failure }
  })
  return { ...e, boxes: fixed, notices }
}

/** Moves a box one place up (-1) or down (1); its links move with it. */
export function moveBox(e: Editor, key: number, dir: -1 | 1): Editor {
  const i = e.boxes.findIndex((b) => b.key === key)
  const j = i + dir
  if (i < 0 || j < 0 || j >= e.boxes.length) return e
  const boxes = [...e.boxes]
  ;[boxes[i], boxes[j]] = [boxes[j], boxes[i]]
  return { ...e, boxes }
}
```

`web/src/lib/types.ts` becomes:
```ts
/** Shapes of the parent API (see `src/manage.rs`). */
export type Direction = 'front_to_back' | 'back_to_front' | 'both'
export type ChildRow = { id: number; name: string; avatar: string; archived: boolean }
export type SubjectRow = { id: number; name: string; archived: boolean }
export type BoxRow = { name: string; interval_days: number; success: number; failure: number }
export type SystemDetail = {
  id: number
  name: string
  preset: boolean
  repeat_missed: boolean
  boxes: BoxRow[]
  decks: number
  is_default: boolean
}
export type BoxInput = BoxRow & { origin: number | null }
export type SystemInput = { name: string; repeat_missed: boolean; boxes: BoxInput[] }
export type DeckInput = {
  name: string
  subject_id: number
  system_id: number
  direction_mode: Direction
  children: number[]
  archived: boolean
}
export type DeckRow = DeckInput & { id: number; cards: number }
export type CardInput = {
  front_text: string
  front_image: number | null
  back_text: string
  back_image: number | null
  notes: string
  active: boolean
}
export type CardRow = CardInput & { id: number }
export type ChildProgress = { child_id: number; name: string; due: number; boxes: number[] }
export type ChildOverview = {
  child_id: number
  name: string
  avatar: string
  due: number
  reviewed: number
  known: number
  not_known: number
  boxes: number[]
}
export type Settings = { name: string; timezone: string; default_system_id: number }
```

`web/src/lib/messages.ts` becomes:
```ts
import { ApiError } from './api'

const FIELD: Record<string, string> = {
  name: 'Geef een naam van hoogstens 50 tekens.',
  avatar: 'Kies een kleur.',
  subject_id: 'Kies een vak.',
  system_id: 'Kies een studiesysteem.',
  direction_mode: 'Kies een richting.',
  children: 'Een van de gekozen kinderen bestaat niet meer.',
  front: 'De voorkant heeft tekst (hoogstens 500 tekens) of een afbeelding nodig.',
  back: 'De achterkant heeft tekst (hoogstens 500 tekens) of een afbeelding nodig.',
  notes: 'Notities zijn hoogstens 1000 tekens.',
  front_image: 'Die afbeelding bestaat niet meer.',
  back_image: 'Die afbeelding bestaat niet meer.',
  image_type: 'Kies een JPEG-, PNG-, WebP- of GIF-afbeelding.',
  image_size: 'Een afbeelding is hoogstens 5 MB.',
  child_id: 'Dat kind oefent dit deck niet.',
  box_name: 'Elk bakje heeft een naam van hoogstens 50 tekens nodig.',
  interval_days: 'Een bakje komt terug na 1 tot 365 dagen.',
  target: 'Elk bakje moet naar een bakje van dit systeem verwijzen.',
  boxes: 'Een studiesysteem heeft 1 tot 12 bakjes.',
  origin: 'Het studiesysteem is intussen gewijzigd. Laad de pagina opnieuw.',
  timezone: 'Kies een tijdzone uit de lijst.',
  default_system_id: 'Kies een bestaand studiesysteem.',
  current_pin: 'De huidige pincode klopt niet.',
  pin: 'De nieuwe pincode bestaat uit 4 tot 8 cijfers.',
}

/** Why the server refused (409), for refusals a parent can act on. */
function refusal(code: string | undefined, count: number): string | null {
  if (code === 'preset') return 'Het vaste studiesysteem kan niet gewijzigd of verwijderd worden. Maak een kopie.'
  if (code === 'default') return 'Dit is het standaardsysteem. Kies eerst een ander systeem als standaard.'
  if (code === 'in_use') return `Dit studiesysteem wordt gebruikt door ${count} ${count === 1 ? 'deck' : 'decks'}.`
  return null
}

/** A Dutch sentence for a failed parent action. */
export function problem(err: unknown): string {
  if (err instanceof ApiError) {
    const field = err.body?.field
    if (field && field in FIELD) return FIELD[field]
    const refused = err.status === 409 ? refusal(err.body?.error, err.body?.count ?? 0) : null
    if (refused) return refused
    if (err.status === 404) return 'Dit bestaat niet meer. Laad de pagina opnieuw.'
  }
  return 'Opslaan lukte niet. Probeer het opnieuw.'
}
```

In `web/src/lib/api.ts`, the error body gains `count`:
```ts
/** Error body the server sends: `{ error, field?, retry_after?, count? }`. */
export type ErrorBody = { error?: string; field?: string; retry_after?: number; count?: number } | null
```
In `DeckForm.svelte`, `Decks.svelte` and `Deck.svelte`, rename `SystemRow` to `SystemDetail` (imports and uses). In `DeckForm.svelte`, the option text becomes `{s.name} ({s.boxes.length} bakjes)`. In `Decks.svelte`, a new deck starts with the family default:
```ts
          system_id: (systems.find((s) => s.is_default) ?? systems[0])?.id ?? 1,
```

- [ ] **Step 4: Run tests and checks**

Run: `npm --prefix web test && npm --prefix web run check`
Expected: 27 web tests pass (8 new); svelte-check 0 errors, 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib web/src/components/DeckForm.svelte web/src/routes/parent/Decks.svelte web/src/routes/parent/Deck.svelte
git commit -m "feat: study system editor model"
```

---

### Task 5: Overview, study systems and settings screens

**Files:**
- Create: `web/src/components/SystemEditor.svelte`, `web/src/routes/parent/Overview.svelte`, `web/src/routes/parent/Systems.svelte`, `web/src/routes/parent/Settings.svelte`
- Modify: `web/src/routes/Parent.svelte`, `web/src/app.css`, `web/src/routes/parent/{Children,Subjects,Decks,Deck}.svelte`

- [ ] **Step 1: The editor and the three tabs**

Create `web/src/components/SystemEditor.svelte`:
```svelte
<script lang="ts">
  import { addBox, moveBox, removeBox, type Editor } from '../lib/systemEditor'

  let {
    editor = $bindable(),
    busy,
    onsave,
    oncancel,
  }: { editor: Editor; busy: boolean; onsave: () => void; oncancel: () => void } = $props()

  function submit(e: SubmitEvent) {
    e.preventDefault()
    onsave()
  }
</script>

<form onsubmit={submit}>
  <label>Naam <input bind:value={editor.name} required /></label>
  <label class="choice">
    <input type="checkbox" bind:checked={editor.repeat_missed} />
    Gemiste kaartjes komen terug in dezelfde oefenbeurt
  </label>

  <div class="scroll">
    <table class="boxes">
      <thead>
        <tr><th>Bakje</th><th>Elke … dagen</th><th>Wist ik →</th><th>Wist ik niet →</th><th></th></tr>
      </thead>
      <tbody>
        {#each editor.boxes as box, i (box.key)}
          <tr>
            <td><input bind:value={box.name} aria-label="Naam van bakje {i + 1}" required /></td>
            <td>
              <input type="number" min="1" max="365" bind:value={box.interval_days} aria-label="Dagen voor {box.name}" required />
            </td>
            <td>
              <select bind:value={box.success} aria-label="{box.name}: na Wist ik">
                {#each editor.boxes as target (target.key)}<option value={target.key}>{target.name}</option>{/each}
              </select>
            </td>
            <td>
              <select bind:value={box.failure} aria-label="{box.name}: na Wist ik niet">
                {#each editor.boxes as target (target.key)}<option value={target.key}>{target.name}</option>{/each}
              </select>
            </td>
            <td class="box-actions">
              <button type="button" class="secondary small" disabled={i === 0} aria-label="{box.name} hoger"
                onclick={() => (editor = moveBox(editor, box.key, -1))}>↑</button>
              <button type="button" class="secondary small" disabled={i === editor.boxes.length - 1} aria-label="{box.name} lager"
                onclick={() => (editor = moveBox(editor, box.key, 1))}>↓</button>
              <button type="button" class="secondary small" disabled={editor.boxes.length === 1} aria-label="{box.name} weghalen"
                onclick={() => (editor = removeBox(editor, box.key))}>✕</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <button type="button" class="secondary" disabled={editor.boxes.length >= 12} onclick={() => (editor = addBox(editor))}>
    + Bakje
  </button>

  {#if editor.notices.length > 0}
    <div class="notices" role="status">
      <p>Door het weghalen van een bakje is dit aangepast:</p>
      <ul>{#each editor.notices as notice, i (i)}<li>{notice}</li>{/each}</ul>
    </div>
  {/if}

  <div class="actions">
    <button type="button" class="secondary" onclick={oncancel}>Annuleer</button>
    <button type="submit" disabled={busy}>Bewaar</button>
  </div>
</form>
```

Create `web/src/routes/parent/Systems.svelte`:
```svelte
<script lang="ts">
  import SystemEditor from '../../components/SystemEditor.svelte'
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import { fromSystem, toInput, type Editor } from '../../lib/systemEditor'
  import type { Settings, SystemDetail } from '../../lib/types'

  let systems = $state<SystemDetail[]>([])
  /** The system being edited (`id`), or a new copy (`id` 0). */
  let editing = $state<{ id: number; editor: Editor } | null>(null)
  let error = $state('')
  let busy = $state(false)

  async function load() {
    systems = await api<SystemDetail[]>('parent/systems')
  }
  load().catch(() => (error = 'De studiesystemen konden niet geladen worden.'))

  async function run(action: () => Promise<unknown>): Promise<boolean> {
    if (busy) return false
    busy = true
    error = ''
    try {
      await action()
      await load()
      return true
    } catch (err) {
      error = problem(err)
      return false
    } finally {
      busy = false
    }
  }

  async function save() {
    if (!editing) return
    const { id, editor } = editing
    const input = toInput(editor)
    const action = () => (id ? api(`parent/systems/${id}`, input, 'PUT') : api('parent/systems', input))
    if (await run(action)) editing = null
  }

  function remove(s: SystemDetail) {
    if (confirm(`Studiesysteem "${s.name}" verwijderen?`)) run(() => api(`parent/systems/${s.id}`, {}, 'DELETE'))
  }

  function makeDefault(s: SystemDetail) {
    run(async () => {
      const settings = await api<Settings>('parent/settings')
      await api('parent/settings', { ...settings, default_system_id: s.id }, 'PUT')
    })
  }

  const days = (n: number) => `${n} ${n === 1 ? 'dag' : 'dagen'}`
  const used = (id: number) => systems.find((s) => s.id === id)?.decks ?? 0
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

{#if editing}
  <section class="card">
    <h2>{editing.id ? 'Studiesysteem wijzigen' : 'Nieuw studiesysteem'}</h2>
    {#if editing.id && used(editing.id) > 0}
      <p class="muted">
        Wijzigingen gelden voor volgende antwoorden; de geplande dagen blijven staan. Kaartjes in
        een weggehaald bakje gaan naar het bakje dat nu op die plaats staat.
      </p>
    {/if}
    <SystemEditor bind:editor={editing.editor} {busy} onsave={save} oncancel={() => (editing = null)} />
  </section>
{:else}
  <ul class="rows">
    {#each systems as s (s.id)}
      <li class="row">
        <span class="grow">
          <strong>{s.name}</strong>{#if s.preset}<span class="tag">vast</span>{/if}{#if s.is_default}<span class="tag">standaard</span>{/if}
          <span class="muted block">
            {s.boxes.map((b) => days(b.interval_days)).join(' → ')} · {s.decks} {s.decks === 1 ? 'deck' : 'decks'}
          </span>
        </span>
        {#if !s.is_default}<button class="secondary" onclick={() => makeDefault(s)}>Maak standaard</button>{/if}
        <button class="secondary" onclick={() => (editing = { id: 0, editor: fromSystem(s, true) })}>Kopieer</button>
        {#if !s.preset}
          <button class="secondary" onclick={() => (editing = { id: s.id, editor: fromSystem(s) })}>Wijzig</button>
          <button class="secondary" onclick={() => remove(s)}>Verwijder</button>
        {/if}
      </li>
    {/each}
  </ul>
  <p class="muted">
    Het vaste systeem volgt de school en kan niet gewijzigd worden. Kopieer het om je eigen
    bakjes en dagen te kiezen. Nieuwe decks krijgen het standaardsysteem.
  </p>
{/if}
```

Create `web/src/routes/parent/Overview.svelte`:
```svelte
<script lang="ts">
  import { api } from '../../lib/api'
  import { initial } from '../../lib/text'
  import type { ChildOverview } from '../../lib/types'

  const overview = api<ChildOverview[]>('parent/overview')
</script>

{#await overview}
  <p class="notice">Laden…</p>
{:then children}
  <div class="overview">
    {#each children as c (c.child_id)}
      {@const most = Math.max(1, ...c.boxes)}
      <section class="card">
        <h2 class="greeting">
          <span class="avatar small" style:background={c.avatar}>{initial(c.name)}</span>
          {c.name}
        </h2>
        <dl class="stats">
          <div><dt>Nog te oefenen</dt><dd>{c.due}</dd></div>
          <div><dt>Vandaag geoefend</dt><dd>{c.reviewed}</dd></div>
          <div><dt>Wist het</dt><dd>{c.known}</dd></div>
          <div><dt>Nog niet</dt><dd>{c.not_known}</dd></div>
        </dl>
        {#if c.boxes.length > 0}
          <h3>Kaartjes per bakje</h3>
          <ul class="box-counts">
            {#each c.boxes as count, i (i)}
              <li>
                <span class="label">Bakje {i + 1}</span>
                <span class="track"><span class="bar" style:width="{(100 * count) / most}%"></span></span>
                <span class="count">{count}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted">Nog geen kaartjes.</p>
        {/if}
      </section>
    {:else}
      <p class="muted">Nog geen kinderen.</p>
    {/each}
  </div>
{:catch}
  <p class="error" role="alert">Het overzicht kon niet geladen worden.</p>
{/await}
```

Create `web/src/routes/parent/Settings.svelte`:
```svelte
<script lang="ts">
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import type { Settings, SystemDetail } from '../../lib/types'

  let settings = $state<Settings | null>(null)
  let systems = $state<SystemDetail[]>([])
  let zones = $state<string[]>([])
  let pins = $state({ current: '', new: '', again: '' })
  let status = $state({ settings: '', pin: '' })
  let errors = $state({ settings: '', pin: '' })
  let busy = $state(false)

  async function load() {
    ;[settings, systems] = await Promise.all([api<Settings>('parent/settings'), api<SystemDetail[]>('parent/systems')])
    zones = Intl.supportedValuesOf('timeZone')
    if (!zones.includes(settings.timezone)) zones = [settings.timezone, ...zones]
  }
  load().catch(() => (errors.settings = 'De instellingen konden niet geladen worden.'))

  async function run(section: 'settings' | 'pin', action: () => Promise<unknown>, done: string) {
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
```

- [ ] **Step 2: Tabs**

`web/src/routes/Parent.svelte` becomes:
```svelte
<script lang="ts">
  import { api, ApiError } from '../lib/api'
  import Children from './parent/Children.svelte'
  import Deck from './parent/Deck.svelte'
  import Decks from './parent/Decks.svelte'
  import Overview from './parent/Overview.svelte'
  import Settings from './parent/Settings.svelte'
  import Subjects from './parent/Subjects.svelte'
  import Systems from './parent/Systems.svelte'

  let { route }: { route: string } = $props()

  // #/ouder (overzicht), #/ouder/<tab>, #/ouder/decks/<id>
  const section = $derived(route.match(/^#\/ouder\/(overzicht|kinderen|vakken|decks|systemen|instellingen)(?:\/(\d+))?/))
  const TABS = [
    ['overzicht', 'Overzicht'],
    ['kinderen', 'Kinderen'],
    ['vakken', 'Vakken'],
    ['decks', 'Decks'],
    ['systemen', 'Studiesystemen'],
    ['instellingen', 'Instellingen'],
  ] as const

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

  // Any parent request the server refuses with 401 means the session ended.
  $effect(() => {
    const locked = () => (unlocked = false)
    addEventListener('wistjet:locked', locked)
    return () => removeEventListener('wistjet:locked', locked)
  })

  async function lock() {
    await api('parent/logout', {}).catch(() => {})
    location.hash = '#/'
  }

  // Lock after 10 minutes without activity; the server enforces the same limit.
  $effect(() => {
    if (!unlocked) return
    let timer = setTimeout(lock, IDLE_MS)
    let lastPing = 0
    const reset = () => {
      clearTimeout(timer)
      timer = setTimeout(lock, IDLE_MS)
      // The server's idle clock only sees requests, so typing a long list would otherwise
      // lock the session mid-sentence. Tell it the parent is still here, at most once a minute.
      if (Date.now() - lastPing > 60_000) {
        lastPing = Date.now()
        api('parent/session').catch(() => {})
      }
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
  <nav class="tabs">
    {#each TABS as [key, label] (key)}
      <a href="#/ouder/{key}" aria-current={(section?.[1] ?? 'overzicht') === key ? 'page' : undefined}>{label}</a>
    {/each}
  </nav>
  {#key route}
    {#if section?.[1] === 'kinderen'}
      <Children />
    {:else if section?.[1] === 'vakken'}
      <Subjects />
    {:else if section?.[1] === 'decks' && section[2]}
      <Deck id={Number(section[2])} />
    {:else if section?.[1] === 'decks'}
      <Decks />
    {:else if section?.[1] === 'systemen'}
      <Systems />
    {:else if section?.[1] === 'instellingen'}
      <Settings />
    {:else}
      <Overview />
    {/if}
  {/key}
{/if}
```

- [ ] **Step 3: Styles and labels**

In `web/src/app.css`:
- the input rule `input:not([type='radio']),` becomes `input:not([type='radio'], [type='checkbox']),`, so checkboxes keep their natural size;
- in `.tabs`, add `overflow-x: auto;`; in `.tabs a`, add `white-space: nowrap;` (six tabs on a phone);
- append:
```css
.block {
  display: block;
}

.scroll {
  overflow-x: auto;
}

.boxes input,
.boxes select {
  margin-top: 0;
  min-width: 90px;
}

.boxes input[type='number'] {
  min-width: 70px;
  width: 80px;
}

.box-actions {
  white-space: nowrap;
}

.box-actions button {
  min-width: 36px;
  padding: 6px 10px;
}

.notices {
  margin: 16px 0 0;
  padding: 12px 16px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--accent) 10%, var(--surface));
}

.notices p,
.notices ul {
  margin: 0;
}

.overview {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}

.overview .card {
  margin: 0;
}

.overview .greeting {
  font-size: 1.3rem;
}

.stats {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
  margin: 0 0 16px;
}

.stats div {
  background: var(--bg);
  border-radius: 12px;
  padding: 10px 12px;
}

.stats dt {
  color: var(--muted);
  font-size: 0.85rem;
}

.stats dd {
  margin: 2px 0 0;
  font-size: 1.6rem;
  font-weight: 800;
}

.box-counts {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: 6px;
}

.box-counts li {
  display: grid;
  grid-template-columns: 4.5em 1fr 2.5em;
  align-items: center;
  gap: 8px;
}

.box-counts .label {
  color: var(--muted);
  font-size: 0.9rem;
}

.box-counts .track {
  height: 10px;
}

.box-counts .bar {
  display: block;
  height: 100%;
  min-width: 2px;
  background: var(--accent);
  border-radius: 0 4px 4px 0;
}

.box-counts .count {
  text-align: right;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

/* A small label after a name: gearchiveerd, uit, vast, standaard. */
.tag {
  display: inline-block;
  margin-left: 8px;
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--line);
  color: var(--muted);
  font-size: 0.8rem;
  font-weight: 600;
  vertical-align: middle;
}
```
In the 4a screens, replace each `<em> · …</em>` separator with a label (Svelte drops the leading space inside it):
- `Children.svelte`: `{#if child.archived}<span class="tag">gearchiveerd</span>{/if}`
- `Subjects.svelte`: `{#if subject.archived}<span class="tag">gearchiveerd</span>{/if}`
- `Decks.svelte`: both `gearchiveerd` separators (subject heading and deck row) → `<span class="tag">gearchiveerd</span>`
- `Deck.svelte`: the heading's `gearchiveerd` → `<span class="tag">gearchiveerd</span>`; the card row's `· uit` → `<span class="tag">uit</span>`

- [ ] **Step 4: Check and build**

Run: `npm --prefix web run check && npm --prefix web test && npm --prefix web run build && cargo test`
Expected: svelte-check 0 errors, 0 warnings; tests pass; build succeeds.

- [ ] **Step 5: Look at it**

Release binary, fresh data folder, setup with the sample deck, let the child answer two cards (one known, one not), unlock the parent area:
- **Overzicht** is the start page: 8 still due, 2 practised today, 1 known, 1 not; Bakje 1 = 9, Bakje 2 = 1.
- **Studiesystemen:** the preset shows the labels "vast" and "standaard". Kopieer: the editor opens on "Kopie van School 4-box". Remove Box 3: a notice says Box 2's "Wist ik" now goes to Box 4. Move Box 4 up. Bewaar, then "Maak standaard": the label moves.
- **Decks:** "Deck toevoegen" now preselects the copy.
- **Deck switched to the copy:** "Verwijder" on the copy says "Dit studiesysteem wordt gebruikt door 1 deck."
- **Instellingen:** rename the family (Bewaard.). A wrong current PIN is refused. Change it to 13579; another unlocked tab now has to unlock again; the new PIN works.
- **Phone width:** the tabs scroll sideways and the box table scrolls inside its card.

- [ ] **Step 6: Commit**

```bash
git add web/src
git commit -m "feat: parent overview, study system editor and settings screens"
```

---

## Done when

- All Rust and web tests pass (4b adds 12 library, 6 API and 8 web tests, and drops one), and the web check and build are clean.
- The Task 5 click-through behaves as listed.
- Spec coverage: §7.1, §7.6, §7.7 (without backup and restore), and the box-removal part of §5.6. Plan 5 adds backup and restore (§9.2) and distribution.
