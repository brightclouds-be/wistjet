# Plan 4a: Parent Content Management — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Behind the PIN, a parent manages children, subjects, decks (subject, study system, direction, assigned children), cards (with an image per side), pastes a list of cards with a preview, sees each child's box distribution per deck, and resets progress.

**Architecture:** `src/manage.rs` holds the queries and validation, with no HTTP. `src/parent_api.rs` maps them to JSON routes and is mounted inside the PIN-guarded `/api/parent` router from Plan 2, so the existing guard covers every new route. Nothing is ever deleted: archive and "off" flags hide things and keep progress (spec §5.8). In the browser, the parent area gets tabs (Kinderen, Vakken, Decks) and a deck page. Paste parsing is a pure function with tests, and every API call that gets a 401 sends the parent back to the PIN screen.

**Tech Stack:** as before; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-08-family-flashcards-design.md` (§4, §5.6 deck system switch, §5.7, §5.8, §7.2–7.5, §8). Roadmap: `docs/superpowers/plans/2026-10-11-wistjet-roadmap.md`.

**Split:** Plan 4 is split in two. This plan (4a) covers content. Plan 4b covers the study-system editor (§7.6, box removal in §5.6), the parent overview (§7.1) and settings (§7.7).

## Decisions made while planning

- **Paste parsing runs in the browser** (`web/src/lib/paste.ts`). It needs the deck's existing fronts, which the deck page already has, and the server re-checks every card when it saves.
- **Saving pasted cards is all or nothing.** One invalid card rejects the batch, so a half-imported list can't happen.
- **Possible duplicates:** a front already in the deck, or earlier in the same paste, counts as a possible duplicate. Matching is exact after trimming, and case-sensitive.
- **Image types come from the file's first bytes** (PNG, JPEG, GIF, WebP), never from the browser's claim. SVG is refused because it can carry script. Uploads are the raw request body, up to 5 MB.
- **Switching a deck's study system** moves progress to `min(box, last box)`. That is `engine::remap_positions` with no kept boxes, done as one SQL `UPDATE`.
- **A new deck uses the first study system** in the list (the preset). The family default becomes selectable in Plan 4b.
- **Box columns are labelled "Bakje 1…n".** Box names arrive with the 4b editor.
- **Reset uses the browser's `confirm()`.** That covers spec §5.7's "requires confirmation" with a native dialog.

## Global Constraints

- Every new route lives under `/api/parent/` and needs a parent session (spec §8).
- Limits (spec §8): names 1–50 characters after trimming; card sides ≤ 500, notes ≤ 1000; images JPEG/PNG/WebP/GIF ≤ 5 MB; a side has text or an image.
- Never delete progress, cards, decks, subjects or children (spec §5.8).
- All interface text is Dutch. Touch targets at least 36px (small buttons) or 48px.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `npm --prefix web run check` (0 errors, 0 warnings) and `npm --prefix web test` clean.

## Review Focus

1. **Unassigning a child, then assigning them again:** progress comes back. Test `unassigning_keeps_progress_for_a_later_return` (Task 1).
2. **A pasted list with one bad line sent anyway** (another tab, an edited request): nothing is saved. Tests `pasted_cards_are_saved_all_or_nothing` (Task 1), `a_bad_pasted_line_saves_nothing` (Task 2).
3. **An SVG or a 6 MB photo uploaded as a card image:** a clear message, nothing stored. Tests `images_are_sniffed_not_trusted` (Task 1), `images_upload_and_show_on_a_card` (Task 2).
4. **The parent session expires while a form is open:** the next save sends them to the PIN screen instead of failing silently. The `wistjet:locked` event in `api.ts` (Task 3); manual check in Task 4.
5. **Editing a deck whose subject or children were archived since:** the form still shows the current choice instead of silently dropping it. The `subjectOptions`/`childOptions` filters in `DeckForm.svelte` (Task 4); manual check.

## File Structure

| File | Responsibility |
|---|---|
| `src/manage.rs` | Content queries + validation: children, subjects, systems (read), decks, cards, images, deck progress, reset |
| `src/parent_api.rs` | JSON routes for the above, mounted in the guarded parent router |
| `src/server.rs` | Mounts `parent_api::routes()`; `error`, `internal`, `today` become `pub(crate)` |
| `tests/api.rs` | + HTTP tests for parent content |
| `web/src/lib/api.ts` | + `method` argument, `upload()`, and a `wistjet:locked` event on parent 401s |
| `web/src/lib/{colours,types,messages,paste}.ts` | Shared palette, API types, Dutch error messages, paste parsing (+ test) |
| `web/src/components/{Swatches,DeckForm,CardForm}.svelte` | Reusable form pieces |
| `web/src/routes/parent/{Children,Subjects,Decks,Deck}.svelte` | The parent screens |
| `web/src/routes/Parent.svelte`, `App.svelte`, `Setup.svelte`, `app.css` | Tabs and sub-routes; Setup reuses Swatches; styles |

API (all need a parent session; errors are `{ error: "invalid", field }` (400) or `not_found` (404)):

| Method & path | Body → result |
|---|---|
| `GET/POST /api/parent/children`, `PUT …/children/{id}` | `{ name, avatar, archived? }` → list / 201 `{ id }` / 204 |
| `GET/POST /api/parent/subjects`, `PUT …/subjects/{id}` | `{ name, archived? }` |
| `GET /api/parent/systems` | `[{ id, name, preset, boxes }]` |
| `GET/POST /api/parent/decks`, `PUT …/decks/{id}` | `{ name, subject_id, system_id, direction_mode, children: [id], archived? }`; list rows add `id` and active `cards` |
| `GET/POST /api/parent/decks/{id}/cards` | POST body is an **array** of `{ front_text, front_image?, back_text, back_image?, notes?, active? }` → 201 `{ ids }` |
| `PUT /api/parent/cards/{id}` | one card → 204 |
| `POST /api/parent/images` | raw image bytes → 201 `{ id }`; fields `image_type`, `image_size` |
| `GET /api/parent/decks/{id}/progress` | `[{ child_id, name, due, boxes: [count per box] }]` |
| `POST /api/parent/decks/{id}/reset` | `{ child_id: id \| null }` → 204 |

Routes in the browser: `#/ouder/kinderen` (default), `#/ouder/vakken`, `#/ouder/decks`, `#/ouder/decks/{id}`.

---

### Task 1: Content data layer

**Files:**
- Create: `src/manage.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `family::{clean_name, valid_avatar}` (Plan 2); `store::{load_boxes, reset_progress, VISIBLE}` (Plans 1, 3)
- Produces: `manage::Error { Invalid(&'static str), NotFound, Db }`, `manage::Result<T>`; the inputs `ChildInput`, `SubjectInput`, `DeckInput`, `CardInput` (`Deserialize`); the rows `ChildRow`, `SubjectRow`, `SystemRow`, `DeckRow`, `CardRow`, `ChildProgress` (`Serialize`); the functions `list_/create_/update_` for children, subjects, decks; `list_systems`; `list_cards`, `create_cards(&mut Connection, deck_id, &[CardInput]) -> Result<Vec<i64>>`, `update_card`; `add_image(&Connection, &[u8]) -> Result<i64>`; `deck_progress(&Connection, deck_id, today)`; `reset(&Connection, deck_id, Option<i64>, today) -> Result<usize>`

- [ ] **Step 1: Write the failing tests**

In `src/lib.rs`, add after `pub mod family;`:
```rust
pub mod manage;
```
Create `src/manage.rs` with the doc comment and the test module:
```rust
//! What the parent manages (spec §7.2–7.4): children, subjects, decks, cards and images.
//! Nothing here deletes: archiving and deactivating hide things and keep progress (§5.8).

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
        }
    }

    fn deck(children: Vec<i64>) -> DeckInput {
        DeckInput {
            name: "Woordjes H1".into(),
            subject_id: 1,
            system_id: 1,
            direction_mode: "front_to_back".into(),
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
            },
        )
        .unwrap();
        assert_eq!(
            list_children(&conn).unwrap()[1],
            ChildRow {
                id: 2,
                name: "Wouter".into(),
                avatar: "#4fc3f7".into(),
                archived: true
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
    fn systems_list_shows_the_preset() {
        let conn = fixture();
        assert_eq!(
            list_systems(&conn).unwrap(),
            vec![SystemRow {
                id: 1,
                name: "School 4-box".into(),
                preset: true,
                boxes: 4
            }]
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
        let cases: [(&str, Break); 5] = [
            ("name", |d| d.name = "".into()),
            ("subject_id", |d| d.subject_id = 9),
            ("system_id", |d| d.system_id = 9),
            ("direction_mode", |d| d.direction_mode = "sideways".into()),
            ("children", |d| d.children = vec![9]),
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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib manage::`
Expected: compile errors, `cannot find function create_child`, `cannot find struct DeckInput`, etc.

- [ ] **Step 3: Implement**

Insert between the doc comment and `#[cfg(test)]` in `src/manage.rs`:
```rust
use crate::family::{clean_name, valid_avatar};
use crate::store::{self, VISIBLE};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, named_params, params};
use serde::{Deserialize, Serialize};

const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    /// Names the offending field, as sent by the client.
    Invalid(&'static str),
    NotFound,
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
}

#[derive(Deserialize)]
pub struct ChildInput {
    pub name: String,
    pub avatar: String,
    #[serde(default)]
    pub archived: bool,
}

fn check_child(c: &ChildInput) -> Result<String> {
    let name = clean_name(&c.name).ok_or(Error::Invalid("name"))?;
    if !valid_avatar(&c.avatar) {
        return Err(Error::Invalid("avatar"));
    }
    Ok(name)
}

/// All children, archived ones too, in the order they were added.
pub fn list_children(conn: &Connection) -> Result<Vec<ChildRow>> {
    let mut stmt = conn.prepare("SELECT id, name, avatar, archived FROM child ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok(ChildRow {
            id: r.get(0)?,
            name: r.get(1)?,
            avatar: r.get(2)?,
            archived: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn create_child(conn: &Connection, c: &ChildInput) -> Result<i64> {
    let name = check_child(c)?;
    conn.execute(
        "INSERT INTO child (name, avatar, archived) VALUES (?1, ?2, ?3)",
        params![name, c.avatar, c.archived],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Renames, recolours, archives or restores a child. Progress is never touched.
pub fn update_child(conn: &Connection, id: i64, c: &ChildInput) -> Result<()> {
    let name = check_child(c)?;
    found(conn.execute(
        "UPDATE child SET name = ?2, avatar = ?3, archived = ?4 WHERE id = ?1",
        params![id, name, c.avatar, c.archived],
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

// ---- study systems (read-only here; the editor is Plan 4b) ----

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SystemRow {
    pub id: i64,
    pub name: String,
    pub preset: bool,
    pub boxes: i64,
}

pub fn list_systems(conn: &Connection) -> Result<Vec<SystemRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.is_preset, (SELECT count(*) FROM box b WHERE b.system_id = s.id)
         FROM study_system s ORDER BY s.is_preset DESC, s.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SystemRow {
            id: r.get(0)?,
            name: r.get(1)?,
            preset: r.get(2)?,
            boxes: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ---- decks ----

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct DeckRow {
    pub id: i64,
    pub name: String,
    pub subject_id: i64,
    pub system_id: i64,
    pub direction_mode: String,
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
    pub children: Vec<i64>,
    #[serde(default)]
    pub archived: bool,
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
                (SELECT count(*) FROM card c WHERE c.deck_id = d.id AND c.active = 1)
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
        "INSERT INTO deck (name, subject_id, system_id, direction_mode, archived)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            name,
            d.subject_id,
            d.system_id,
            d.direction_mode,
            d.archived
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
                         archived = ?6
         WHERE id = ?1",
        params![
            id,
            name,
            d.subject_id,
            d.system_id,
            d.direction_mode,
            d.archived
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
```

Notes:
- `check_*` runs before any transaction, so the write lock is only held for the writes.
- `set_children` replaces the assignment set. Deleting `deck_child` rows only unlinks; `progress` rows stay (spec §5.8).
- `deck_progress` counts directions without a progress row as the first box, because that's where a new card direction starts (Plan 1).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: 14 new tests pass; fmt and clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/manage.rs src/lib.rs
git commit -m "feat: parent content management queries"
```

---

### Task 2: Parent content API

**Files:**
- Create: `src/parent_api.rs`
- Modify: `src/server.rs`, `src/lib.rs`, `tests/api.rs`

**Interfaces:**
- Consumes: Task 1; `server::Shared`
- Produces: `parent_api::routes() -> Router<Shared>` (paths relative to `/api/parent`); the API table above. `server::{error, internal, today}` become `pub(crate)` for reuse.

- [ ] **Step 1: Write the failing tests**

Append to `tests/api.rs`:
```rust
// ---- parent content management (Plan 4a) ----

/// A set-up family with a logged-in parent: (router, cookie).
async fn parent() -> (Router, String) {
    let app = app();
    set_up(&app).await;
    let cookie = log_in(&app).await;
    (app, cookie)
}

#[tokio::test]
async fn content_routes_need_a_parent_session() {
    let app = app();
    set_up(&app).await;
    let routes = [
        ("GET", "/api/parent/children"),
        ("POST", "/api/parent/children"),
        ("PUT", "/api/parent/children/1"),
        ("GET", "/api/parent/subjects"),
        ("GET", "/api/parent/systems"),
        ("GET", "/api/parent/decks"),
        ("PUT", "/api/parent/decks/1"),
        ("GET", "/api/parent/decks/1/cards"),
        ("POST", "/api/parent/decks/1/cards"),
        ("GET", "/api/parent/decks/1/progress"),
        ("POST", "/api/parent/decks/1/reset"),
        ("PUT", "/api/parent/cards/1"),
        ("POST", "/api/parent/images"),
    ];
    for (method, path) in routes {
        let r = call(&app, method, path, None, Some(json!({}))).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn parent_adds_renames_and_archives_a_child() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/children",
        Some(&c),
        Some(json!({ "name": "Wout", "avatar": "#4fc3f7" })),
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (StatusCode::CREATED, json!({ "id": 2 }))
    );
    let body = json!({ "name": "Wouter", "avatar": "#4fc3f7", "archived": true });
    let r = call(&app, "PUT", "/api/parent/children/2", Some(&c), Some(body)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/children", Some(&c), None).await;
    assert_eq!(
        r.json()[1],
        json!({ "id": 2, "name": "Wouter", "avatar": "#4fc3f7", "archived": true })
    );
    // Archived children are gone from the profile screen.
    let r = call(&app, "GET", "/api/children", None, None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_content_names_the_field() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/children",
        Some(&c),
        Some(json!({ "name": "Wout", "avatar": "blue" })),
    )
    .await;
    assert_eq!(
        (r.status, r.json()),
        (
            StatusCode::BAD_REQUEST,
            json!({ "error": "invalid", "field": "avatar" })
        )
    );
    let r = call(
        &app,
        "PUT",
        "/api/parent/children/99",
        Some(&c),
        Some(json!({ "name": "X", "avatar": "#4fc3f7" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn parent_builds_a_deck_with_cards() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/parent/subjects",
        Some(&c),
        Some(json!({ "name": "Wiskunde" })),
    )
    .await;
    let subject = r.json()["id"].clone();
    let deck = json!({ "name": "Tafels", "subject_id": subject, "system_id": 1, "direction_mode": "both", "children": [1] });
    let r = call(&app, "POST", "/api/parent/decks", Some(&c), Some(deck)).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let deck_id = r.json()["id"].as_i64().unwrap();

    let cards = json!([
        { "front_text": "3 x 4", "back_text": "12", "notes": "Denk aan 4 + 4 + 4" },
        { "front_text": "6 x 7", "back_text": "42" }
    ]);
    let path = format!("/api/parent/decks/{deck_id}/cards");
    let r = call(&app, "POST", &path, Some(&c), Some(cards)).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let first = r.json()["ids"][0].as_i64().unwrap();

    let off = json!({ "front_text": "3 x 4", "back_text": "12", "active": false });
    let r = call(
        &app,
        "PUT",
        &format!("/api/parent/cards/{first}"),
        Some(&c),
        Some(off),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    let r = call(&app, "GET", &path, Some(&c), None).await;
    assert_eq!(r.json()[0]["active"], json!(false));
    let r = call(&app, "GET", "/api/parent/decks", Some(&c), None).await;
    let tafels = r
        .json()
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == deck_id)
        .unwrap()
        .clone();
    assert_eq!(
        (tafels["cards"].clone(), tafels["children"].clone()),
        (json!(1), json!([1]))
    );
    // Both directions of the one active card are now due for Lotte, next to the 10 sample cards.
    let r = call(&app, "GET", "/api/children/1/home", None, None).await;
    assert_eq!(r.json()["due"], json!(12));
}

#[tokio::test]
async fn a_bad_pasted_line_saves_nothing() {
    let (app, c) = parent().await;
    let cards = json!([{ "front_text": "un", "back_text": "een" }, { "front_text": "deux", "back_text": " " }]);
    let r = call(
        &app,
        "POST",
        "/api/parent/decks/1/cards",
        Some(&c),
        Some(cards),
    )
    .await;
    assert_eq!(
        (r.status, r.json()["field"].clone()),
        (StatusCode::BAD_REQUEST, json!("back"))
    );
    let r = call(&app, "GET", "/api/parent/decks/1/cards", Some(&c), None).await;
    assert_eq!(r.json().as_array().unwrap().len(), 10);
}

async fn upload(app: &Router, cookie: &str, bytes: Vec<u8>) -> Reply {
    let req = Request::builder()
        .method("POST")
        .uri("/api/parent/images")
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/octet-stream")
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
async fn images_upload_and_show_on_a_card() {
    let (app, c) = parent().await;
    let png = b"\x89PNG\r\n\x1a\nnot really a picture".to_vec();
    let r = upload(&app, &c, png.clone()).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let id = r.json()["id"].as_i64().unwrap();
    let card = json!({ "front_text": "", "front_image": id, "back_text": "de kat" });
    let r = call(&app, "PUT", "/api/parent/cards/1", Some(&c), Some(card)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", &format!("/api/images/{id}"), None, None).await;
    assert_eq!((r.content_type.as_str(), r.body), ("image/png", png));

    let r = upload(&app, &c, b"<svg onload=alert(1)>".to_vec()).await;
    assert_eq!(
        r.json(),
        json!({ "error": "invalid", "field": "image_type" })
    );
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(5 * 1024 * 1024 + 1, 0);
    let r = upload(&app, &c, big).await;
    assert_eq!(
        r.json(),
        json!({ "error": "invalid", "field": "image_size" })
    );
}

#[tokio::test]
async fn deck_progress_and_reset() {
    let (app, c) = parent().await;
    let r = call(
        &app,
        "POST",
        "/api/answers",
        None,
        Some(answer_body("a1", 1)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/decks/1/progress", Some(&c), None).await;
    assert_eq!(
        r.json(),
        json!([{ "child_id": 1, "name": "Lotte", "due": 9, "boxes": [9, 1, 0, 0] }])
    );

    let r = call(
        &app,
        "POST",
        "/api/parent/decks/1/reset",
        Some(&c),
        Some(json!({ "child_id": 1 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(&app, "GET", "/api/parent/decks/1/progress", Some(&c), None).await;
    assert_eq!(r.json()[0]["boxes"], json!([10, 0, 0, 0]));

    let r = call(
        &app,
        "POST",
        "/api/parent/decks/99/reset",
        Some(&c),
        Some(json!({})),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix web run build && cargo test --test api`
Expected: the new tests fail. Routes that don't exist yet fall through to the static fallback's JSON 404 (the guard's `route_layer` only wraps routes that exist), so even the session test gets 404 instead of 401.

- [ ] **Step 3: Implement**

Create `src/parent_api.rs`:
```rust
//! Parent routes for content management (spec §7.2–7.4). `server::router` mounts these
//! inside the PIN-guarded `/api/parent` router, so every one needs a parent session.

use crate::manage::{self, CardInput, ChildInput, DeckInput, Error, SubjectInput};
use crate::server::{Shared, error, internal, today};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
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
        .route("/systems", get(list_systems))
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
    ok(manage::list_systems(&s.db.lock().unwrap()))
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

async fn add_image(State(s): State<Shared>, body: Bytes) -> Response {
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

In `src/lib.rs`, add after `pub mod manage;`:
```rust
pub mod parent_api;
```
In `src/server.rs`:
- in `router`, add `.merge(crate::parent_api::routes())` right after `.route("/session", get(|| async { StatusCode::NO_CONTENT }))`, so the routes sit before `.route_layer(...)` and the guard covers them;
- make three helpers crate-visible: `fn error(` → `pub(crate) fn error(`, `fn internal(` → `pub(crate) fn internal(`, `fn today(` → `pub(crate) fn today(`.

`axum` limits request bodies to 2 MB by default; the `/images` route raises that to 6 MB, so `add_image` can answer a too-big image with its own `image_size` error.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all pass (7 new in `api`); fmt and clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/parent_api.rs src/server.rs src/lib.rs tests/api.rs
git commit -m "feat: parent API for children, subjects, decks, cards and images"
```

---

### Task 3: Web building blocks

**Files:**
- Create: `web/src/lib/paste.ts`, `web/src/lib/paste.test.ts`, `web/src/lib/colours.ts`, `web/src/lib/types.ts`, `web/src/lib/messages.ts`, `web/src/components/Swatches.svelte`
- Modify: `web/src/lib/api.ts`, `web/src/routes/Setup.svelte`

**Interfaces:**
- Produces: `parsePaste(text, existingFronts) -> Pasted[]`; `api(path, body?, method = 'POST')`; `upload(path, file)`; window event `wistjet:locked`; `COLOURS`; the API types; `problem(err) -> string`; `<Swatches bind:value name>`

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/paste.test.ts`:
```ts
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parsePaste } from './paste.ts'

const pairs = (text: string, existing: string[] = []) =>
  parsePaste(text, existing).map((p) => [p.front, p.back, p.problem, p.duplicate])

test('a tab wins over " - " and " = "', () => {
  assert.deepEqual(pairs('a - b\tc = d'), [['a - b', 'c = d', null, false]])
})

test('" - " wins over " = ", and only the first one splits', () => {
  assert.deepEqual(pairs('a = b - c - d'), [['a = b', 'c - d', null, false]])
  assert.deepEqual(pairs('le chat = de kat'), [['le chat', 'de kat', null, false]])
})

test('hyphens without spaces do not split', () => {
  assert.deepEqual(pairs("l'arc-en-ciel"), [["l'arc-en-ciel", '', 'split', false]])
})

test('empty sides are flagged', () => {
  assert.deepEqual(pairs('le chat -  \n = de kat'), [
    ['le chat', '', 'empty', false],
    ['', 'de kat', 'empty', false],
  ])
})

test('fronts already in the deck or earlier in the paste are possible duplicates', () => {
  assert.deepEqual(pairs('le chat\tde kat\nle chien\tde hond\nle chien\tde hond', ['le chat']), [
    ['le chat', 'de kat', null, true],
    ['le chien', 'de hond', null, false],
    ['le chien', 'de hond', null, true],
  ])
})

test('blank lines are skipped and Windows line ends are fine', () => {
  const lines = parsePaste('un\teen\r\n\r\n  \r\ndeux\tTwee\r\n', [])
  assert.deepEqual(lines.map((l) => [l.line, l.front, l.back]), [[1, 'un', 'een'], [4, 'deux', 'Twee']])
})

test('sides over 500 characters are flagged', () => {
  assert.equal(parsePaste(`x\t${'é'.repeat(501)}`, [])[0].problem, 'long')
  assert.equal(parsePaste(`x\t${'é'.repeat(500)}`, [])[0].problem, null)
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npm --prefix web test`
Expected: FAIL, `Cannot find module …/paste.ts`.

- [ ] **Step 3: Implement**

Create `web/src/lib/paste.ts`:
```ts
/** One non-blank pasted line (spec §7.5). Lines with a `problem` are not saved;
 *  possible duplicates are, unless the parent unticks them. */
export type Pasted = {
  line: number
  front: string
  back: string
  problem: 'split' | 'empty' | 'long' | null
  duplicate: boolean
}

// In order of preference: the first separator kind found in a line splits it once.
const SEPARATORS = ['\t', ' - ', ' = ']

export function parsePaste(text: string, existingFronts: string[]): Pasted[] {
  const seen = new Set(existingFronts.map((f) => f.trim()))
  const out: Pasted[] = []
  text.split(/\r?\n/).forEach((raw, i) => {
    if (!raw.trim()) return
    const sep = SEPARATORS.find((s) => raw.includes(s))
    if (!sep) {
      out.push({ line: i + 1, front: raw.trim(), back: '', problem: 'split', duplicate: false })
      return
    }
    const at = raw.indexOf(sep)
    const front = raw.slice(0, at).trim()
    const back = raw.slice(at + sep.length).trim()
    const long = [...front].length > 500 || [...back].length > 500
    const problem = !front || !back ? 'empty' : long ? 'long' : null
    const duplicate = problem === null && seen.has(front)
    if (problem === null) seen.add(front)
    out.push({ line: i + 1, front, back, problem, duplicate })
  })
  return out
}
```

`web/src/lib/api.ts` becomes:
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

/** GET `/api/<path>`, or send `body` as JSON with `method` (POST unless given). */
export function api<T = unknown>(path: string, body?: unknown, method = 'POST'): Promise<T> {
  const init: RequestInit =
    body === undefined
      ? {}
      : { method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }
  return request<T>(path, init)
}

/** POSTs a file as the raw request body (card images). */
export function upload<T = unknown>(path: string, file: Blob): Promise<T> {
  return request<T>(path, { method: 'POST', body: file })
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

Create `web/src/lib/colours.ts`:
```ts
/** Avatar colours (the server accepts any `#rrggbb`; these are the ones offered). */
export const COLOURS = ['#e57373', '#f06292', '#ba68c8', '#7986cb', '#4fc3f7', '#4db6ac', '#aed581', '#ffb74d']
```

Create `web/src/lib/types.ts`:
```ts
/** Shapes of the parent API (see `src/manage.rs`). */
export type Direction = 'front_to_back' | 'back_to_front' | 'both'
export type ChildRow = { id: number; name: string; avatar: string; archived: boolean }
export type SubjectRow = { id: number; name: string; archived: boolean }
export type SystemRow = { id: number; name: string; preset: boolean; boxes: number }
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
```

Create `web/src/lib/messages.ts`:
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
}

/** A Dutch sentence for a failed parent action. */
export function problem(err: unknown): string {
  if (err instanceof ApiError) {
    const field = err.body?.field
    if (field && field in FIELD) return FIELD[field]
    if (err.status === 404) return 'Dit bestaat niet meer. Laad de pagina opnieuw.'
  }
  return 'Opslaan lukte niet. Probeer het opnieuw.'
}
```

Create `web/src/components/Swatches.svelte`:
```svelte
<script lang="ts">
  import { COLOURS } from '../lib/colours'

  let { value = $bindable(), name }: { value: string; name: string } = $props()
</script>

<fieldset class="swatches">
  <legend>Kleur</legend>
  {#each COLOURS as colour, i (colour)}
    <label class="swatch" style:background={colour}>
      <input type="radio" {name} value={colour} bind:group={value} />
      <span class="sr-only">Kleur {i + 1}</span>
    </label>
  {/each}
</fieldset>
```

In `web/src/routes/Setup.svelte`:
- replace `import { api, ApiError } from '../lib/api'` with:
```ts
  import Swatches from '../components/Swatches.svelte'
  import { api, ApiError } from '../lib/api'
  import { COLOURS } from '../lib/colours'
```
- delete the line `const COLOURS = [...]`;
- replace the whole `<fieldset class="swatches"> … </fieldset>` block with:
```svelte
      <Swatches bind:value={childAvatar} name="colour" />
```

- [ ] **Step 4: Run tests and checks**

Run: `npm --prefix web test && npm --prefix web run check`
Expected: 19 web tests pass (7 new); svelte-check 0 errors, 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib web/src/components web/src/routes/Setup.svelte
git commit -m "feat: paste parsing and shared web building blocks"
```

---

### Task 4: Parent screens

**Files:**
- Create: `web/src/components/DeckForm.svelte`, `web/src/components/CardForm.svelte`, `web/src/routes/parent/Children.svelte`, `web/src/routes/parent/Subjects.svelte`, `web/src/routes/parent/Decks.svelte`, `web/src/routes/parent/Deck.svelte`
- Modify: `web/src/routes/Parent.svelte`, `web/src/App.svelte`, `web/src/app.css`

**Interfaces:**
- Consumes: Task 2 routes; Task 3 building blocks

- [ ] **Step 1: Form components**

Create `web/src/components/DeckForm.svelte`. The parent owns the `deck` object through `bind:deck`, so the form never copies props:
```svelte
<script lang="ts">
  import type { ChildRow, DeckInput, SubjectRow, SystemRow } from '../lib/types'

  let {
    deck = $bindable(),
    subjects,
    systems,
    children,
    submitLabel,
    onsubmit,
  }: {
    deck: DeckInput
    subjects: SubjectRow[]
    systems: SystemRow[]
    children: ChildRow[]
    submitLabel: string
    onsubmit: () => void
  } = $props()

  const DIRECTIONS = [
    ['front_to_back', 'Voorkant → achterkant'],
    ['back_to_front', 'Achterkant → voorkant'],
    ['both', 'Beide richtingen (elk apart geoefend)'],
  ] as const

  // Archived subjects and children are only offered when this deck already uses them.
  const subjectOptions = $derived(subjects.filter((s) => !s.archived || s.id === deck.subject_id))
  const childOptions = $derived(children.filter((c) => !c.archived || deck.children.includes(c.id)))

  function submit(e: SubmitEvent) {
    e.preventDefault()
    onsubmit()
  }
</script>

<form onsubmit={submit}>
  <label>Naam <input bind:value={deck.name} required /></label>
  <label>
    Vak
    <select bind:value={deck.subject_id} required>
      {#each subjectOptions as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
    </select>
  </label>
  <label>
    Studiesysteem
    <select bind:value={deck.system_id}>
      {#each systems as s (s.id)}<option value={s.id}>{s.name} ({s.boxes} bakjes)</option>{/each}
    </select>
  </label>
  <fieldset class="choices">
    <legend>Richting</legend>
    {#each DIRECTIONS as [value, label] (value)}
      <label class="choice"><input type="radio" name="direction" {value} bind:group={deck.direction_mode} /> {label}</label>
    {/each}
  </fieldset>
  <fieldset class="choices">
    <legend>Wie oefent dit deck?</legend>
    {#each childOptions as c (c.id)}
      <label class="choice"><input type="checkbox" value={c.id} bind:group={deck.children} /> {c.name}</label>
    {:else}
      <p class="muted">Nog geen kinderen.</p>
    {/each}
  </fieldset>
  <div class="actions"><button type="submit">{submitLabel}</button></div>
</form>
```

Create `web/src/components/CardForm.svelte`:
```svelte
<script lang="ts">
  import { upload } from '../lib/api'
  import { problem } from '../lib/messages'
  import type { CardInput } from '../lib/types'

  let {
    card = $bindable(),
    submitLabel,
    onsubmit,
    oncancel,
  }: { card: CardInput; submitLabel: string; onsubmit: () => void; oncancel?: () => void } = $props()

  const MAX_BYTES = 5 * 1024 * 1024
  let uploadError = $state('')
  let uploading = $state(false)

  async function pick(side: 'front' | 'back', e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    uploadError = ''
    if (file.size > MAX_BYTES) {
      uploadError = 'Een afbeelding is hoogstens 5 MB.'
      return
    }
    uploading = true
    try {
      const { id } = await upload<{ id: number }>('parent/images', file)
      if (side === 'front') card.front_image = id
      else card.back_image = id
    } catch (err) {
      uploadError = problem(err)
    } finally {
      uploading = false
    }
  }

  function submit(e: SubmitEvent) {
    e.preventDefault()
    onsubmit()
  }
</script>

{#snippet image(id: number | null, side: 'front' | 'back', remove: () => void)}
  {#if id !== null}
    <img class="thumb" src="/api/images/{id}" alt="" />
    <button type="button" class="secondary small" onclick={remove}>Afbeelding weg</button>
  {:else}
    <label class="file">
      + Afbeelding
      <input type="file" accept="image/jpeg,image/png,image/webp,image/gif" onchange={(e) => pick(side, e)} />
    </label>
  {/if}
{/snippet}

<form class="card-form" onsubmit={submit}>
  <div class="sides">
    <fieldset class="side">
      <legend>Voorkant</legend>
      <input bind:value={card.front_text} aria-label="Tekst voorkant" />
      {@render image(card.front_image, 'front', () => (card.front_image = null))}
    </fieldset>
    <fieldset class="side">
      <legend>Achterkant</legend>
      <input bind:value={card.back_text} aria-label="Tekst achterkant" />
      {@render image(card.back_image, 'back', () => (card.back_image = null))}
    </fieldset>
  </div>
  <label>Notities (optioneel) <input bind:value={card.notes} /></label>
  {#if uploadError}<p class="error" role="alert">{uploadError}</p>{/if}
  <div class="actions">
    {#if oncancel}<button type="button" class="secondary" onclick={oncancel}>Annuleer</button>{/if}
    <button type="submit" disabled={uploading}>{submitLabel}</button>
  </div>
</form>
```

- [ ] **Step 2: Children and subjects**

Create `web/src/routes/parent/Children.svelte`:
```svelte
<script lang="ts">
  import Swatches from '../../components/Swatches.svelte'
  import { api } from '../../lib/api'
  import { COLOURS } from '../../lib/colours'
  import { problem } from '../../lib/messages'
  import { initial } from '../../lib/text'
  import type { ChildRow } from '../../lib/types'

  let children = $state<ChildRow[]>([])
  let draft = $state({ name: '', avatar: COLOURS[0] })
  /** The child being edited; id 0 = none. */
  let edit = $state({ id: 0, name: '', avatar: '' })
  let error = $state('')

  async function load() {
    children = await api<ChildRow[]>('parent/children')
  }
  load()

  async function run(action: () => Promise<unknown>): Promise<boolean> {
    error = ''
    try {
      await action()
      await load()
      return true
    } catch (err) {
      error = problem(err)
      return false
    }
  }

  async function add(e: SubmitEvent) {
    e.preventDefault()
    if (await run(() => api('parent/children', draft))) draft = { name: '', avatar: COLOURS[0] }
  }

  async function save(e: SubmitEvent, child: ChildRow) {
    e.preventDefault()
    const body = { name: edit.name, avatar: edit.avatar, archived: child.archived }
    if (await run(() => api(`parent/children/${child.id}`, body, 'PUT'))) edit.id = 0
  }

  const setArchived = (child: ChildRow, archived: boolean) =>
    run(() => api(`parent/children/${child.id}`, { ...child, archived }, 'PUT'))
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

<ul class="rows">
  {#each children as child (child.id)}
    <li class="row" class:archived={child.archived}>
      {#if edit.id === child.id}
        <form class="grow" onsubmit={(e) => save(e, child)}>
          <label>Naam <input bind:value={edit.name} required /></label>
          <Swatches bind:value={edit.avatar} name="edit-child" />
          <div class="actions">
            <button type="button" class="secondary" onclick={() => (edit.id = 0)}>Annuleer</button>
            <button type="submit">Bewaar</button>
          </div>
        </form>
      {:else}
        <span class="avatar small" style:background={child.avatar}>{initial(child.name)}</span>
        <span class="grow">{child.name}{#if child.archived}<em> · gearchiveerd</em>{/if}</span>
        <button class="secondary" onclick={() => (edit = { id: child.id, name: child.name, avatar: child.avatar })}>
          Wijzig
        </button>
        <button class="secondary" onclick={() => setArchived(child, !child.archived)}>
          {child.archived ? 'Zet terug' : 'Archiveer'}
        </button>
      {/if}
    </li>
  {/each}
</ul>

<section class="card">
  <h2>Kind toevoegen</h2>
  <form onsubmit={add}>
    <label>Naam <input bind:value={draft.name} required /></label>
    <Swatches bind:value={draft.avatar} name="new-child" />
    <div class="actions"><button type="submit">Toevoegen</button></div>
  </form>
</section>
```

Create `web/src/routes/parent/Subjects.svelte`:
```svelte
<script lang="ts">
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import type { SubjectRow } from '../../lib/types'

  let subjects = $state<SubjectRow[]>([])
  let draft = $state('')
  /** The subject being renamed; id 0 = none. */
  let edit = $state({ id: 0, name: '' })
  let error = $state('')

  async function load() {
    subjects = await api<SubjectRow[]>('parent/subjects')
  }
  load()

  async function run(action: () => Promise<unknown>): Promise<boolean> {
    error = ''
    try {
      await action()
      await load()
      return true
    } catch (err) {
      error = problem(err)
      return false
    }
  }

  async function add(e: SubmitEvent) {
    e.preventDefault()
    if (await run(() => api('parent/subjects', { name: draft }))) draft = ''
  }

  async function save(e: SubmitEvent, subject: SubjectRow) {
    e.preventDefault()
    const body = { name: edit.name, archived: subject.archived }
    if (await run(() => api(`parent/subjects/${subject.id}`, body, 'PUT'))) edit.id = 0
  }

  const setArchived = (subject: SubjectRow, archived: boolean) =>
    run(() => api(`parent/subjects/${subject.id}`, { ...subject, archived }, 'PUT'))
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

<ul class="rows">
  {#each subjects as subject (subject.id)}
    <li class="row" class:archived={subject.archived}>
      {#if edit.id === subject.id}
        <form class="grow inline" onsubmit={(e) => save(e, subject)}>
          <input bind:value={edit.name} aria-label="Naam van het vak" required />
          <button type="button" class="secondary" onclick={() => (edit.id = 0)}>Annuleer</button>
          <button type="submit">Bewaar</button>
        </form>
      {:else}
        <span class="grow">{subject.name}{#if subject.archived}<em> · gearchiveerd</em>{/if}</span>
        <button class="secondary" onclick={() => (edit = { id: subject.id, name: subject.name })}>Wijzig</button>
        <button class="secondary" onclick={() => setArchived(subject, !subject.archived)}>
          {subject.archived ? 'Zet terug' : 'Archiveer'}
        </button>
      {/if}
    </li>
  {:else}
    <li class="muted">Nog geen vakken.</li>
  {/each}
</ul>

<section class="card">
  <h2>Vak toevoegen</h2>
  <form class="inline" onsubmit={add}>
    <input bind:value={draft} aria-label="Naam van het vak" placeholder="Bijvoorbeeld Frans" required />
    <button type="submit">Toevoegen</button>
  </form>
</section>
```

- [ ] **Step 3: Decks and the deck page**

Create `web/src/routes/parent/Decks.svelte`. After adding a deck it jumps to the deck page to add cards:
```svelte
<script lang="ts">
  import DeckForm from '../../components/DeckForm.svelte'
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import type { ChildRow, DeckInput, DeckRow, SubjectRow, SystemRow } from '../../lib/types'

  let decks = $state<DeckRow[]>([])
  let subjects = $state<SubjectRow[]>([])
  let systems = $state<SystemRow[]>([])
  let children = $state<ChildRow[]>([])
  let draft = $state<DeckInput | null>(null)
  let error = $state('')

  async function load() {
    ;[decks, subjects, systems, children] = await Promise.all([
      api<DeckRow[]>('parent/decks'),
      api<SubjectRow[]>('parent/subjects'),
      api<SystemRow[]>('parent/systems'),
      api<ChildRow[]>('parent/children'),
    ])
    const subject = subjects.find((s) => !s.archived)
    draft = subject
      ? {
          name: '',
          subject_id: subject.id,
          system_id: systems[0]?.id ?? 1,
          direction_mode: 'front_to_back',
          children: children.filter((c) => !c.archived).map((c) => c.id),
          archived: false,
        }
      : null
  }
  load()

  async function add() {
    if (!draft) return
    error = ''
    try {
      const { id } = await api<{ id: number }>('parent/decks', draft)
      location.hash = `#/ouder/decks/${id}`
    } catch (err) {
      error = problem(err)
    }
  }

  const names = (ids: number[]) =>
    children.filter((c) => ids.includes(c.id)).map((c) => c.name).join(', ') || 'niemand'
</script>

{#each subjects as subject (subject.id)}
  {@const own = decks.filter((d) => d.subject_id === subject.id)}
  {#if own.length > 0}
    <h2>{subject.name}{#if subject.archived}<em class="muted"> · gearchiveerd</em>{/if}</h2>
    <ul class="rows">
      {#each own as deck (deck.id)}
        <li class="row" class:archived={deck.archived}>
          <a class="grow deck-link" href="#/ouder/decks/{deck.id}">
            <strong>{deck.name}</strong>{#if deck.archived}<em> · gearchiveerd</em>{/if}
            <span class="muted">{deck.cards} kaartjes · {names(deck.children)}</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
{/each}

<section class="card">
  <h2>Deck toevoegen</h2>
  {#if draft}
    <DeckForm bind:deck={draft} {subjects} {systems} {children} submitLabel="Toevoegen" onsubmit={add} />
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  {:else}
    <p>Maak eerst een vak aan bij <a href="#/ouder/vakken">Vakken</a>.</p>
  {/if}
</section>
```

Create `web/src/routes/parent/Deck.svelte`:
```svelte
<script lang="ts">
  import CardForm from '../../components/CardForm.svelte'
  import DeckForm from '../../components/DeckForm.svelte'
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import { parsePaste, type Pasted } from '../../lib/paste'
  import type { CardInput, CardRow, ChildProgress, ChildRow, DeckInput, DeckRow, SubjectRow, SystemRow } from '../../lib/types'

  let { id }: { id: number } = $props()

  const blankCard = (): CardInput => ({
    front_text: '',
    front_image: null,
    back_text: '',
    back_image: null,
    notes: '',
    active: true,
  })

  let deck = $state<DeckRow | null>(null)
  let missing = $state(false)
  let subjects = $state<SubjectRow[]>([])
  let systems = $state<SystemRow[]>([])
  let children = $state<ChildRow[]>([])
  let cards = $state<CardRow[]>([])
  let progress = $state<ChildProgress[]>([])

  let settings = $state<DeckInput | null>(null)
  let newCard = $state(blankCard())
  /** The card being edited, as a form copy; `editId` 0 = none. */
  let editId = $state(0)
  let editCard = $state(blankCard())
  let pasteText = $state('')
  let preview = $state<(Pasted & { keep: boolean })[] | null>(null)

  let errors = $state({ cards: '', paste: '', progress: '', settings: '' })
  let saved = $state('')

  async function load() {
    const [decks, s, sy, ch] = await Promise.all([
      api<DeckRow[]>('parent/decks'),
      api<SubjectRow[]>('parent/subjects'),
      api<SystemRow[]>('parent/systems'),
      api<ChildRow[]>('parent/children'),
    ])
    deck = decks.find((d) => d.id === id) ?? null
    missing = deck === null
    if (!deck) return
    subjects = s
    systems = sy
    children = ch
    const { name, subject_id, system_id, direction_mode, children: assigned, archived } = deck
    settings = { name, subject_id, system_id, direction_mode, children: [...assigned], archived }
    ;[cards, progress] = await Promise.all([
      api<CardRow[]>(`parent/decks/${id}/cards`),
      api<ChildProgress[]>(`parent/decks/${id}/progress`),
    ])
  }

  $effect(() => {
    load()
  })

  /** Runs a change, reloads, and reports a failure in the given section. */
  async function run(section: keyof typeof errors, action: () => Promise<unknown>): Promise<boolean> {
    errors[section] = ''
    saved = ''
    try {
      await action()
      await load()
      return true
    } catch (err) {
      errors[section] = problem(err)
      return false
    }
  }

  async function addCard() {
    if (await run('cards', () => api(`parent/decks/${id}/cards`, [newCard]))) newCard = blankCard()
  }

  function startEdit(card: CardRow) {
    const { id: cardId, ...input } = card
    editId = cardId
    editCard = input
  }

  async function saveEdit() {
    if (await run('cards', () => api(`parent/cards/${editId}`, editCard, 'PUT'))) editId = 0
  }

  const setActive = (card: CardRow, active: boolean) => {
    const { id: cardId, ...input } = card
    return run('cards', () => api(`parent/cards/${cardId}`, { ...input, active }, 'PUT'))
  }

  function showPreview() {
    preview = parsePaste(pasteText, cards.map((c) => c.front_text)).map((p) => ({ ...p, keep: p.problem === null }))
  }

  const toAdd = $derived(preview?.filter((p) => p.problem === null && p.keep) ?? [])

  async function addPasted() {
    const batch = toAdd.map((p) => ({ ...blankCard(), front_text: p.front, back_text: p.back }))
    if (await run('paste', () => api(`parent/decks/${id}/cards`, batch))) {
      saved = `${batch.length} kaartjes toegevoegd.`
      pasteText = ''
      preview = null
    }
  }

  function reset(child: ChildProgress | null) {
    const who = child ? child.name : 'iedereen'
    if (!confirm(`Alle kaartjes van dit deck terug naar bakje 1 voor ${who}? De geschiedenis blijft bewaard.`)) return
    run('progress', () => api(`parent/decks/${id}/reset`, { child_id: child?.child_id ?? null }))
  }

  async function saveSettings() {
    if (settings && (await run('settings', () => api(`parent/decks/${id}`, settings, 'PUT')))) saved = 'Bewaard.'
  }

  const setArchived = (archived: boolean) =>
    settings && run('settings', () => api(`parent/decks/${id}`, { ...settings, archived }, 'PUT'))

  const PROBLEM = { split: 'geen scheiding gevonden', empty: 'voor- of achterkant leeg', long: 'langer dan 500 tekens' }
</script>

{#snippet side(text: string, image: number | null)}
  <span class="card-side">
    {#if image !== null}<img class="thumb" src="/api/images/{image}" alt="" />{/if}
    {text}
  </span>
{/snippet}

<p><a href="#/ouder/decks">← Alle decks</a></p>

{#if missing}
  <p class="card">Dit deck bestaat niet.</p>
{:else if deck && settings}
  <h2 class="deck-title">{deck.name}{#if deck.archived}<em class="muted"> · gearchiveerd</em>{/if}</h2>
  {#if saved}<p class="saved" role="status">{saved}</p>{/if}

  <section class="card">
    <h3>Kaartjes ({cards.filter((c) => c.active).length})</h3>
    <ul class="rows">
      {#each cards as card (card.id)}
        <li class="row" class:archived={!card.active}>
          {#if editId === card.id}
            <div class="grow">
              <CardForm bind:card={editCard} submitLabel="Bewaar" onsubmit={saveEdit} oncancel={() => (editId = 0)} />
            </div>
          {:else}
            <span class="grow card-line">
              {@render side(card.front_text, card.front_image)}
              <span class="muted">→</span>
              {@render side(card.back_text, card.back_image)}
              {#if card.notes}<span class="muted note">{card.notes}</span>{/if}
              {#if !card.active}<em> · uit</em>{/if}
            </span>
            <button class="secondary" onclick={() => startEdit(card)}>Wijzig</button>
            <button class="secondary" onclick={() => setActive(card, !card.active)}>
              {card.active ? 'Zet uit' : 'Zet aan'}
            </button>
          {/if}
        </li>
      {:else}
        <li class="muted">Nog geen kaartjes.</li>
      {/each}
    </ul>
    <h3>Kaartje toevoegen</h3>
    <CardForm bind:card={newCard} submitLabel="Toevoegen" onsubmit={addCard} />
    {#if errors.cards}<p class="error" role="alert">{errors.cards}</p>{/if}
  </section>

  <section class="card">
    <h3>Lijst plakken</h3>
    <p class="muted">
      Eén kaartje per regel: voorkant en achterkant gescheiden door een tab, " - " of " = ".
      Afbeeldingen voeg je daarna per kaartje toe.
    </p>
    <textarea bind:value={pasteText} rows="6" aria-label="Geplakte lijst" placeholder={'le chat - de kat\nle chien - de hond'}></textarea>
    <div class="actions"><button class="secondary" onclick={showPreview} disabled={!pasteText.trim()}>Bekijk</button></div>
    {#if preview}
      <table class="preview">
        <thead><tr><th></th><th>Voorkant</th><th>Achterkant</th><th></th></tr></thead>
        <tbody>
          {#each preview as row (row.line)}
            <tr class:skipped={row.problem !== null}>
              <td>
                {#if row.problem === null}
                  <input type="checkbox" bind:checked={row.keep} aria-label="Regel {row.line} toevoegen" />
                {/if}
              </td>
              <td>{row.front}</td>
              <td>{row.back}</td>
              <td class="muted">
                {#if row.problem}regel {row.line}: {PROBLEM[row.problem]}{:else if row.duplicate}staat er misschien al in{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="actions">
        <button onclick={addPasted} disabled={toAdd.length === 0}>Voeg {toAdd.length} kaartjes toe</button>
      </div>
    {/if}
    {#if errors.paste}<p class="error" role="alert">{errors.paste}</p>{/if}
  </section>

  <section class="card">
    <h3>Voortgang</h3>
    {#if progress.length === 0}
      <p class="muted">Niemand oefent dit deck.</p>
    {:else}
      <table class="progress-table">
        <thead>
          <tr>
            <th>Kind</th><th>Vandaag</th>
            {#each progress[0].boxes as _, i (i)}<th>Bakje {i + 1}</th>{/each}
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each progress as child (child.child_id)}
            <tr>
              <td>{child.name}</td>
              <td>{child.due}</td>
              {#each child.boxes as count, i (i)}<td>{count}</td>{/each}
              <td><button class="secondary small" onclick={() => reset(child)}>Opnieuw beginnen</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="actions">
        <button class="secondary" onclick={() => reset(null)}>Opnieuw beginnen voor iedereen</button>
      </div>
    {/if}
    {#if errors.progress}<p class="error" role="alert">{errors.progress}</p>{/if}
  </section>

  <section class="card">
    <h3>Instellingen</h3>
    <DeckForm bind:deck={settings} {subjects} {systems} {children} submitLabel="Bewaar" onsubmit={saveSettings} />
    {#if errors.settings}<p class="error" role="alert">{errors.settings}</p>{/if}
    <div class="actions">
      <button class="secondary" onclick={() => setArchived(!deck!.archived)}>
        {deck.archived ? 'Zet terug' : 'Archiveer dit deck'}
      </button>
    </div>
  </section>
{:else}
  <p class="notice">Laden…</p>
{/if}
```

- [ ] **Step 4: Tabs, routes and styles**

`web/src/routes/Parent.svelte` becomes:
```svelte
<script lang="ts">
  import { api, ApiError } from '../lib/api'
  import Children from './parent/Children.svelte'
  import Deck from './parent/Deck.svelte'
  import Decks from './parent/Decks.svelte'
  import Subjects from './parent/Subjects.svelte'

  let { route }: { route: string } = $props()

  // #/ouder/kinderen, #/ouder/vakken, #/ouder/decks, #/ouder/decks/<id>
  const section = $derived(route.match(/^#\/ouder\/(kinderen|vakken|decks)(?:\/(\d+))?/))
  const TABS = [
    ['kinderen', 'Kinderen'],
    ['vakken', 'Vakken'],
    ['decks', 'Decks'],
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
  <nav class="tabs">
    {#each TABS as [key, label] (key)}
      <a href="#/ouder/{key}" aria-current={(section?.[1] ?? 'kinderen') === key ? 'page' : undefined}>{label}</a>
    {/each}
  </nav>
  {#key route}
    {#if section?.[1] === 'vakken'}
      <Subjects />
    {:else if section?.[1] === 'decks' && section[2]}
      <Deck id={Number(section[2])} />
    {:else if section?.[1] === 'decks'}
      <Decks />
    {:else}
      <Children />
    {/if}
  {/key}
{/if}
```

In `web/src/App.svelte`, the parent route now covers its sub-routes. Replace:
```svelte
  {:else if route === '#/ouder'}
    <Parent />
```
with:
```svelte
  {:else if route.startsWith('#/ouder')}
    <Parent {route} />
```
Append to `web/src/app.css`:
```css
.tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 20px;
  border-bottom: 1px solid var(--line);
}

.tabs a {
  padding: 10px 16px;
  color: var(--muted);
  font-weight: 700;
  text-decoration: none;
  border-bottom: 3px solid transparent;
  margin-bottom: -1px;
}

.tabs a[aria-current='page'] {
  color: var(--ink);
  border-bottom-color: var(--accent);
}

.rows {
  list-style: none;
  padding: 0;
  margin: 0 0 20px;
  display: grid;
  gap: 8px;
}

.row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  padding: 12px 14px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 14px;
}

.row.archived {
  opacity: 0.6;
}

.grow {
  flex: 1;
  min-width: 0;
}

.muted {
  color: var(--muted);
}

.inline {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}

.inline input {
  flex: 1;
  min-width: 160px;
  margin-top: 0;
}

.card h2,
.card h3 {
  margin: 0 0 12px;
}

section.card {
  margin-bottom: 20px;
}

.deck-link {
  display: flex;
  flex-direction: column;
  color: inherit;
  text-decoration: none;
}

.choices {
  border: none;
  padding: 0;
  margin: 0 0 16px;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.choices legend {
  font-weight: 600;
  margin-bottom: 8px;
}

.choices .choice {
  margin: 0;
  align-items: center;
}

.card-form .sides {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
  margin-bottom: 12px;
}

.side {
  border: 1px solid var(--line);
  border-radius: 12px;
  padding: 10px 12px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-start;
}

.side legend {
  font-weight: 600;
  padding: 0 4px;
}

.side input:not([type='file']) {
  margin-top: 0;
}

.thumb {
  max-height: 56px;
  max-width: 96px;
  border-radius: 8px;
  vertical-align: middle;
}

.file {
  font-weight: 600;
  color: var(--accent);
  cursor: pointer;
  margin: 0;
}

.file input {
  position: absolute;
  opacity: 0;
  width: 1px;
  height: 1px;
}

.file:has(input:focus-visible) {
  outline: 3px solid color-mix(in srgb, var(--accent) 50%, transparent);
  outline-offset: 2px;
}

button.small {
  min-height: 36px;
  padding: 6px 12px;
  font-size: 0.9rem;
}

.card-line {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.card-side {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
}

.note {
  font-size: 0.9rem;
}

textarea {
  width: 100%;
  font: inherit;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: 12px;
}

table {
  width: 100%;
  border-collapse: collapse;
  margin: 12px 0;
}

th,
td {
  text-align: left;
  padding: 8px;
  border-bottom: 1px solid var(--line);
}

tr.skipped td {
  color: var(--muted);
  text-decoration: line-through;
}

tr.skipped td:last-child {
  text-decoration: none;
}

.progress-table {
  display: block;
  overflow-x: auto;
}

.saved {
  color: var(--yes);
  font-weight: 700;
}
```

- [ ] **Step 5: Check and build**

Run: `npm --prefix web run check && npm --prefix web test && npm --prefix web run build && cargo test`
Expected: svelte-check 0 errors, 0 warnings; web tests pass; build succeeds; all Rust tests pass.

- [ ] **Step 6: Look at it**

Run the release binary with a fresh data folder, finish setup with the sample deck, open the lock icon, enter the PIN, then check:
- **Kinderen:** add "Wout" with a colour; rename him; archive him (gone from the profile screen); restore him.
- **Vakken:** add "Wiskunde", rename it, archive it, restore it.
- **Decks:** the sample deck is listed under Frans. Add a deck: it opens its page.
- **Deck page, cards:** add a card; edit a card and attach an image to its front (it shows as a thumbnail in the list); switch a card off and on.
- **Paste a list:** paste `le chat - de kat`, `la fleur - de bloem`, `zonder scheiding`, `le ciel = de lucht`. "Bekijk" marks le chat as a possible duplicate (ticked), strikes through "zonder scheiding", and offers 3; untick le chat, add 2; the card count rises by 2.
- **Voortgang:** after the child answers a card, the table shows the box spread; "Opnieuw beginnen" asks first, then everything is in Bakje 1.
- **Instellingen:** switch to "Beide richtingen" and the child's due count doubles; archive the deck and it leaves the child's home.
- **Session end:** wait 10 minutes (or restart the server) and press a save button: the PIN screen appears.
- **Upload limits:** try a `.svg` and a file over 5 MB: each gets a clear message.

- [ ] **Step 7: Commit**

```bash
git add web/src
git commit -m "feat: parent screens for children, subjects, decks and cards"
```

---

## Done when

- All Rust and web tests pass (4a adds 14 library, 7 API and 7 web tests), and the web check and build are clean.
- The Task 4 click-through behaves as listed.
- Spec coverage: §7.2, §7.3, §7.4 (deck page incl. box distribution and due count), §7.5, §5.7 reset, §5.6 system switch on a deck, §5.8 hiding and restoring. Plan 4b: §7.1 overview, §7.6 study systems (box removal), §7.7 settings.
