# Plan 3: Child Experience — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A child taps their profile, sees what is due today, studies their cards (all or one subject) with reveal and "Wist ik / Wist ik niet", missed cards come back when the study system says so, and a summary closes the session.

**Architecture:** `src/study.rs` builds the child home counts and the study queue from Plan 1's store (`due_items`, the `VISIBLE` join) and orders it with `engine::order_session`. `src/server.rs` exposes them, plus answers (`store::record_answer`) and card images, without a PIN (spec §8 allows children exactly this). The browser owns the session: `web/src/lib/session.ts` is a pure queue (first showings are saved, repeats of missed cards are practice and never sent), and the Svelte screens render it.

**Tech Stack:** as Plan 2. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-08-family-flashcards-design.md` (§5.4, §5.5, §6, §8). Roadmap: `docs/superpowers/plans/2026-10-11-wistjet-roadmap.md`.

## Decisions made while planning

- **The server orders, the browser queues.** `GET /api/children/{id}/session` returns the due cards in study order (oldest due first, ties shuffled, directions of one card apart). The browser appends missed cards and counts the summary.
- **Repeats are never sent.** The server records only the first showing of each card direction; repeats are practice (spec §5.4: not logged, only counted in the summary). The server would ignore them anyway, so this saves requests without changing outcomes.
- **One retry path.** A failed save keeps the card on screen with a single "Opnieuw proberen" button that resends the *same* answer under the *same* id, so a retry can't change the answer or count twice.
- **Answer ids use `crypto.getRandomValues`.** `crypto.randomUUID` only exists in secure contexts, and plain `http://` on the home network isn't one.
- **"Due at start of day"** for the progress bar is `answered_today + due`, where `answered_today` counts today's scheduling answers in the review log.
- **Card images have empty `alt` text.** The data model has no alt text; the card text, when present, sits next to the image.

## Global Constraints

- All interface text is Dutch. Children never see boxes, intervals or dates (spec §6.3); the only date shown is "when the next cards come" on the home screen (spec §6.2).
- Child routes need no PIN; they may only list children, read a child's home and session, submit answers, and fetch card images (spec §8).
- Never drop an answer silently (spec §6.3, §10).
- Touch targets at least 48px; answer buttons larger. Keyboard: space reveals, ← = not known, → = known.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `npm --prefix web run check` (0 errors, 0 warnings) and `npm --prefix web test` clean.

## Review Focus

1. **A save fails mid-session** (wifi blip): the card stays, "Opnieuw proberen" resends the same answer id, and the server counts it once. Tests `answer_counts_once_and_moves_the_home_counts` (Task 2); manual check in Task 4 (stop the server mid-session).
2. **Space bar while an answer button has focus:** must not "click" the button. `onkeydown` always prevents the default for space. Manual check in Task 4.
3. **A missed card when repeats are off:** it leaves the session. Tests `missed cards leave when the study system does not repeat them` (Task 3), `session_follows_the_repeat_missed_setting` (Task 1).
4. **A profile archived while its home is open,** or a typed-in URL for a child that doesn't exist: friendly message, not a crash. Tests `unknown_or_archived_child_has_no_home` (Task 1), `unknown_child_has_no_home` (Task 2).
5. **Nothing due when the session starts** (another tab finished the cards): the session ends at once with "Niets geoefend". Test `answering past the end changes nothing` (Task 3); manual check in Task 4.

## File Structure

| File | Responsibility |
|---|---|
| `src/study.rs` | Child home counts, the oriented study queue, image lookup |
| `src/family.rs` | + `timezone()`, which decides "today" |
| `src/store.rs` | `VISIBLE` becomes `pub(crate)` so `study` reuses the visibility rules |
| `src/server.rs` | + child home, session, answers and image routes; profile tiles get `due` |
| `tests/api.rs` | + HTTP tests for the above |
| `web/src/lib/session.ts` | Pure study queue: start, answer, summary, answer ids |
| `web/src/lib/text.ts` | + `whenNext()` for "De volgende kaartjes komen …" |
| `web/src/routes/ChildHome.svelte`, `Study.svelte` | Child home; study and summary |
| `web/src/routes/Profiles.svelte`, `App.svelte`, `app.css` | Tiles become links with due badges; routes; styles |

API added (no PIN):

| Method & path | Result |
|---|---|
| `GET /api/children` | now also `due` per child |
| `GET /api/children/{id}/home` | `{ name, avatar, due, subjects: [{ id, name, due }], answered_today, next_due }`; 404 for unknown/archived |
| `GET /api/children/{id}/session[?subject=ID]` | `[{ card_id, direction, subject, deck, question: { text, image }, answer: { text, image }, notes, repeat_missed }]` in study order |
| `POST /api/answers` | body `{ id, child_id, card_id, direction, known }`; 204 (scheduled, practice or repeat); 404 not visible; 400 malformed; 409 before setup |
| `GET /api/images/{id}` | image bytes with their type; 404 |

Routes: `#/kind/{id}` (home), `#/kind/{id}/oefenen` (all due), `#/kind/{id}/oefenen/{subject}`.

---

### Task 1: Child home and study queries

**Files:**
- Create: `src/study.rs`
- Modify: `src/family.rs`, `src/store.rs`, `src/lib.rs`

**Interfaces:**
- Consumes: `store::due_items`, `store::record_answer`, the `VISIBLE` SQL fragment (Plan 1); `engine::order_session` (Plan 1)
- Produces: `family::timezone(&Connection) -> rusqlite::Result<Option<chrono_tz::Tz>>`; `study::home(&Connection, child_id: i64, today: NaiveDate) -> rusqlite::Result<Option<Home>>`; `study::session<R: Rng>(&Connection, child_id: i64, subject_id: Option<i64>, today: NaiveDate, &mut R) -> rusqlite::Result<Vec<StudyCard>>`; `study::image(&Connection, id: i64) -> rusqlite::Result<Option<(String, Vec<u8>)>>`; the `Serialize` structs `Home`, `SubjectDue`, `StudyCard`, `Side` (their JSON is the API table above)

- [ ] **Step 1: Write the failing tests**

In `src/family.rs`, add to the `tests` module before `fn archived_children_are_not_listed`:
```rust
    #[test]
    fn timezone_is_known_after_setup() {
        let mut conn = db::open_in_memory().unwrap();
        assert_eq!(timezone(&conn).unwrap(), None);
        create(&mut conn, &setup(false)).unwrap();
        assert_eq!(timezone(&conn).unwrap(), Some(chrono_tz::Europe::Brussels));
    }
```

In `src/store.rs`, change `const VISIBLE: &str = "` to:
```rust
pub(crate) const VISIBLE: &str = "
```
In `src/lib.rs`, add after `pub mod store;`:
```rust
pub mod study;
```
Create `src/study.rs` with the doc comment and the test module:
```rust
//! What a child sees (spec §6): home counts, the study queue, and card images.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::store::{Answer, record_answer};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    const TODAY: &str = "2026-10-17";

    /// Lotte has French (2 cards) and maths (1 card, with notes); Wout only French.
    fn fixture() -> Connection {
        let conn = db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO child (id, name, avatar) VALUES (1, 'Lotte', '#e57373'), (2, 'Wout', '#64b5f6');
             INSERT INTO subject (id, name) VALUES (1, 'Frans'), (2, 'Wiskunde');
             INSERT INTO deck (id, subject_id, name, system_id, direction_mode) VALUES
               (1, 1, 'Woordjes H1', 1, 'front_to_back'),
               (2, 2, 'Tafels', 1, 'front_to_back');
             INSERT INTO deck_child (deck_id, child_id) VALUES (1, 1), (2, 1), (1, 2);
             INSERT INTO card (id, deck_id, front_text, back_text, notes) VALUES
               (1, 1, 'le chat', 'de kat', ''),
               (2, 1, 'le chien', 'de hond', ''),
               (3, 2, '3 x 4', '12', 'Denk aan 4 + 4 + 4');",
        )
        .unwrap();
        conn
    }

    fn answer(conn: &mut Connection, id: &str, card_id: i64) {
        let a = Answer {
            id: id.into(),
            child_id: 1,
            card_id,
            direction: 0,
            known: true,
        };
        let now = "2026-10-17T09:00:00Z".parse().unwrap();
        record_answer(conn, &a, now, d(TODAY)).unwrap();
    }

    fn session_for(conn: &Connection, subject: Option<i64>) -> Vec<StudyCard> {
        session(conn, 1, subject, d(TODAY), &mut StdRng::seed_from_u64(0)).unwrap()
    }

    #[test]
    fn home_counts_due_cards_per_subject() {
        let conn = fixture();
        let h = home(&conn, 1, d(TODAY)).unwrap().unwrap();
        assert_eq!(h.name, "Lotte");
        assert_eq!(h.due, 3);
        assert_eq!(
            h.subjects,
            vec![
                SubjectDue {
                    id: 1,
                    name: "Frans".into(),
                    due: 2
                },
                SubjectDue {
                    id: 2,
                    name: "Wiskunde".into(),
                    due: 1
                },
            ]
        );
        assert_eq!(h.answered_today, 0);
        assert_eq!(h.next_due, None);
    }

    #[test]
    fn answering_moves_the_progress_bar() {
        let mut conn = fixture();
        answer(&mut conn, "a1", 1);
        let h = home(&conn, 1, d(TODAY)).unwrap().unwrap();
        assert_eq!((h.answered_today, h.due), (1, 2));
    }

    #[test]
    fn nothing_due_tells_when_the_next_cards_come() {
        let mut conn = fixture();
        for card in 1..=3 {
            answer(&mut conn, &format!("a{card}"), card);
        }
        let h = home(&conn, 1, d(TODAY)).unwrap().unwrap();
        assert_eq!(h.due, 0);
        assert!(h.subjects.is_empty());
        assert_eq!(h.next_due.as_deref(), Some("2026-10-19")); // box 2 = 2 days
    }

    #[test]
    fn unknown_or_archived_child_has_no_home() {
        let conn = fixture();
        assert_eq!(home(&conn, 99, d(TODAY)).unwrap(), None);
        conn.execute_batch("UPDATE child SET archived = 1 WHERE id = 2")
            .unwrap();
        assert_eq!(home(&conn, 2, d(TODAY)).unwrap(), None);
    }

    #[test]
    fn session_turns_cards_by_direction() {
        let conn = fixture();
        let card = |cards: &[StudyCard]| {
            cards
                .iter()
                .find(|c| c.card_id == 1)
                .map(|c| (c.question.text.clone(), c.answer.text.clone()))
        };
        assert_eq!(
            card(&session_for(&conn, Some(1))),
            Some(("le chat".into(), "de kat".into()))
        );
        conn.execute_batch("UPDATE deck SET direction_mode = 'back_to_front' WHERE id = 1")
            .unwrap();
        assert_eq!(
            card(&session_for(&conn, Some(1))),
            Some(("de kat".into(), "le chat".into()))
        );
    }

    #[test]
    fn session_for_one_subject_carries_deck_and_notes() {
        let conn = fixture();
        let cards = session_for(&conn, Some(2));
        assert_eq!(
            cards,
            vec![StudyCard {
                card_id: 3,
                direction: 0,
                subject: "Wiskunde".into(),
                deck: "Tafels".into(),
                question: Side {
                    text: "3 x 4".into(),
                    image: None
                },
                answer: Side {
                    text: "12".into(),
                    image: None
                },
                notes: "Denk aan 4 + 4 + 4".into(),
                repeat_missed: true,
            }]
        );
        assert_eq!(session_for(&conn, None).len(), 3);
    }

    #[test]
    fn session_follows_the_repeat_missed_setting() {
        let conn = fixture();
        conn.execute_batch("UPDATE study_system SET repeat_missed = 0")
            .unwrap();
        assert!(session_for(&conn, None).iter().all(|c| !c.repeat_missed));
    }

    #[test]
    fn image_is_returned_with_its_type() {
        let conn = fixture();
        conn.execute_batch(
            "INSERT INTO image (id, mime, data) VALUES (1, 'image/png', x'89504E47')",
        )
        .unwrap();
        assert_eq!(
            image(&conn, 1).unwrap(),
            Some(("image/png".into(), vec![0x89, 0x50, 0x4E, 0x47]))
        );
        assert_eq!(image(&conn, 2).unwrap(), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib`
Expected: compile errors, `cannot find function timezone`, `cannot find function home`, `cannot find struct StudyCard`, etc.

- [ ] **Step 3: Implement**

In `src/family.rs`, add before `/// Active children for the profile screen`:
```rust
/// The family's timezone, which decides what "today" is (spec §5). `None` before setup.
pub fn timezone(conn: &Connection) -> rusqlite::Result<Option<Tz>> {
    let tz: Option<String> = conn
        .query_row("SELECT timezone FROM family", [], |r| r.get(0))
        .optional()?;
    Ok(tz.and_then(|t| t.parse().ok()))
}
```

In `src/study.rs`, insert between the doc comment and `#[cfg(test)]`:
```rust
use crate::engine;
use crate::store::{self, VISIBLE};
use chrono::NaiveDate;
use rand::Rng;
use rusqlite::{Connection, OptionalExtension, Result, named_params, params};
use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SubjectDue {
    pub id: i64,
    pub name: String,
    pub due: i64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Home {
    pub name: String,
    pub avatar: String,
    pub due: i64,
    pub subjects: Vec<SubjectDue>,
    /// Scheduling answers given today; `answered_today / (answered_today + due)` is the
    /// day's progress bar (answered vs. due at start of day).
    pub answered_today: i64,
    /// When nothing is due: the next day something is (`YYYY-MM-DD`).
    pub next_due: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Side {
    pub text: String,
    pub image: Option<i64>,
}

/// One due card direction, already turned so `question` is the side shown first.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct StudyCard {
    pub card_id: i64,
    pub direction: u8,
    pub subject: String,
    pub deck: String,
    pub question: Side,
    pub answer: Side,
    pub notes: String,
    /// The deck's study system setting (spec §5.4); the browser repeats missed cards.
    pub repeat_missed: bool,
}

/// The child home screen (spec §6.2). `None` for an unknown or archived child.
pub fn home(conn: &Connection, child_id: i64, today: NaiveDate) -> Result<Option<Home>> {
    let child = conn
        .query_row(
            "SELECT name, avatar FROM child WHERE id = ?1 AND archived = 0",
            [child_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((name, avatar)) = child else {
        return Ok(None);
    };
    let sql = format!(
        "SELECT s.id, s.name, count(*) {VISIBLE}
           AND COALESCE(p.due_date, :today) <= :today
         GROUP BY s.id ORDER BY s.name"
    );
    let subjects = conn
        .prepare(&sql)?
        .query_map(named_params! { ":child": child_id, ":today": today }, |r| {
            Ok(SubjectDue {
                id: r.get(0)?,
                name: r.get(1)?,
                due: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    let due = subjects.iter().map(|s| s.due).sum();
    let answered_today = conn.query_row(
        "SELECT count(*) FROM review_log WHERE child_id = ?1 AND local_date = ?2",
        params![child_id, today],
        |r| r.get(0),
    )?;
    // With nothing due, every visible direction has a progress row due after today.
    let next_due = if due == 0 {
        conn.query_row(
            &format!("SELECT min(p.due_date) {VISIBLE}"),
            named_params! { ":child": child_id },
            |r| r.get(0),
        )?
    } else {
        None
    };
    Ok(Some(Home {
        name,
        avatar,
        due,
        subjects,
        answered_today,
        next_due,
    }))
}

/// The cards for one study session (spec §5.5), in study order.
// ponytail: one query per card; a session is at most a few hundred cards.
pub fn session<R: Rng>(
    conn: &Connection,
    child_id: i64,
    subject_id: Option<i64>,
    today: NaiveDate,
    rng: &mut R,
) -> Result<Vec<StudyCard>> {
    let items = engine::order_session(store::due_items(conn, child_id, subject_id, today)?, rng);
    let mut stmt = conn.prepare(
        "SELECT s.name, d.name, c.front_text, c.front_image_id, c.back_text, c.back_image_id,
                c.notes, ss.repeat_missed
         FROM card c
         JOIN deck d ON d.id = c.deck_id
         JOIN subject s ON s.id = d.subject_id
         JOIN study_system ss ON ss.id = d.system_id
         WHERE c.id = ?1",
    )?;
    items
        .iter()
        .map(|item| {
            stmt.query_row([item.card_id], |r| {
                let front = Side {
                    text: r.get(2)?,
                    image: r.get(3)?,
                };
                let back = Side {
                    text: r.get(4)?,
                    image: r.get(5)?,
                };
                let (question, answer) = if item.direction == 0 {
                    (front, back)
                } else {
                    (back, front)
                };
                Ok(StudyCard {
                    card_id: item.card_id,
                    direction: item.direction,
                    subject: r.get(0)?,
                    deck: r.get(1)?,
                    question,
                    answer,
                    notes: r.get(6)?,
                    repeat_missed: r.get(7)?,
                })
            })
        })
        .collect()
}

/// An uploaded card image: its media type and bytes.
pub fn image(conn: &Connection, id: i64) -> Result<Option<(String, Vec<u8>)>> {
    conn.query_row("SELECT mime, data FROM image WHERE id = ?1", [id], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })
    .optional()
}
```

`home` groups the due query by subject, so a subject with nothing due is left out and "Of kies een vak" only offers subjects with work. `next_due` is only computed when nothing is due: then every visible direction has a progress row (rows without one are due today), so `min(p.due_date)` is the next day with cards.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: 9 new tests pass (8 in `study`, 1 in `family`); fmt and clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/study.rs src/family.rs src/store.rs src/lib.rs
git commit -m "feat: child home counts and study queue"
```

---

### Task 2: Child API

**Files:**
- Modify: `src/server.rs`, `tests/api.rs`

**Interfaces:**
- Consumes: Task 1; `store::{Answer, AnswerOutcome, record_answer, due_items}`; `engine::local_today`
- Produces: the routes in the API table

- [ ] **Step 1: Write the failing tests**

In `tests/api.rs`, the profile tiles now carry `due`. In `setup_lists_the_first_child`, change the expected JSON to:
```rust
        json!([{ "id": 1, "name": "Lotte", "avatar": "#e57373", "due": 10 }])
```
Append to `tests/api.rs`:
```rust
fn answer_body(id: &str, card_id: i64) -> Value {
    json!({ "id": id, "child_id": 1, "card_id": card_id, "direction": 0, "known": true })
}

#[tokio::test]
async fn child_home_shows_what_is_due() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "GET", "/api/children/1/home", None, None).await;
    assert_eq!(
        r.json(),
        json!({
            "name": "Lotte",
            "avatar": "#e57373",
            "due": 10,
            "subjects": [{ "id": 1, "name": "Frans", "due": 10 }],
            "answered_today": 0,
            "next_due": null
        })
    );
}

#[tokio::test]
async fn unknown_child_has_no_home() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "GET", "/api/children/99/home", None, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn session_lists_due_cards_question_first() {
    let app = app();
    set_up(&app).await;
    let r = call(&app, "GET", "/api/children/1/session", None, None).await;
    let cards = r.json();
    assert_eq!(cards.as_array().unwrap().len(), 10);
    let chat = cards
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["card_id"] == 1)
        .unwrap();
    assert_eq!(
        chat["question"],
        json!({ "text": "le chat", "image": null })
    );
    assert_eq!(chat["answer"], json!({ "text": "de kat", "image": null }));
    assert_eq!(chat["subject"], json!("Frans"));
    assert_eq!(chat["repeat_missed"], json!(true));

    let r = call(
        &app,
        "GET",
        "/api/children/1/session?subject=99",
        None,
        None,
    )
    .await;
    assert_eq!(r.json(), json!([]));
}

#[tokio::test]
async fn answer_counts_once_and_moves_the_home_counts() {
    let app = app();
    set_up(&app).await;
    for _ in 0..2 {
        let r = call(
            &app,
            "POST",
            "/api/answers",
            None,
            Some(answer_body("a1", 1)),
        )
        .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
    }
    let r = call(&app, "GET", "/api/children/1/home", None, None).await;
    assert_eq!(r.json()["answered_today"], json!(1));
    assert_eq!(r.json()["due"], json!(9));
}

#[tokio::test]
async fn answer_for_a_card_the_child_cannot_see_is_not_found() {
    let app = app();
    set_up(&app).await;
    let r = call(
        &app,
        "POST",
        "/api/answers",
        None,
        Some(answer_body("a1", 999)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn malformed_answers_are_rejected() {
    let app = app();
    set_up(&app).await;
    let mut no_id = answer_body("", 1);
    no_id["id"] = json!("");
    let mut long_id = answer_body("", 1);
    long_id["id"] = json!("x".repeat(65));
    let mut bad_direction = answer_body("a1", 1);
    bad_direction["direction"] = json!(2);
    for body in [no_id, long_id, bad_direction] {
        let r = call(&app, "POST", "/api/answers", None, Some(body)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn answer_before_setup_is_a_conflict() {
    let app = app();
    let r = call(
        &app,
        "POST",
        "/api/answers",
        None,
        Some(answer_body("a1", 1)),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn card_images_are_served_with_their_type() {
    let (app, state) = app_and_state();
    state
        .db
        .lock()
        .unwrap()
        .execute_batch("INSERT INTO image (id, mime, data) VALUES (1, 'image/png', x'89504E47')")
        .unwrap();
    let r = call(&app, "GET", "/api/images/1", None, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.content_type, "image/png");
    assert_eq!(r.body, vec![0x89, 0x50, 0x4E, 0x47]);
    let r = call(&app, "GET", "/api/images/2", None, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test api`
Expected: the new tests fail (404 from the unknown routes; `setup_lists_the_first_child` misses `due`).

- [ ] **Step 3: Implement**

In `src/server.rs`, replace the `use` lines from `use crate::auth::{self, Auth, Login};` up to (not including) `use serde::Deserialize;` with:
```rust
use crate::auth::{self, Auth, Login};
use crate::family::{self, Setup, SetupError};
use crate::store::{self, Answer, AnswerOutcome};
use crate::{engine, study};
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{NaiveDate, Utc};
use rusqlite::Connection;
```
In `router`, add after `.route("/api/children", get(children))`:
```rust
        .route("/api/children/{id}/home", get(child_home))
        .route("/api/children/{id}/session", get(child_session))
        .route("/api/answers", post(answer))
        .route("/api/images/{id}", get(image))
```
Replace the whole `async fn children` handler with:
```rust
/// The family's calendar day right now; `None` before setup.
fn today(conn: &Connection) -> rusqlite::Result<Option<NaiveDate>> {
    Ok(family::timezone(conn)?.map(|tz| engine::local_today(tz, Utc::now())))
}

/// Profile tiles with their due badge (spec §6.1).
async fn children(State(s): State<Shared>) -> Response {
    let conn = s.db.lock().unwrap();
    let tiles = (|| {
        let today = today(&conn)?;
        family::children(&conn)?
            .into_iter()
            .map(|c| {
                let due = match today {
                    Some(t) => store::due_items(&conn, c.id, None, t)?.len(),
                    None => 0,
                };
                Ok(json!({ "id": c.id, "name": c.name, "avatar": c.avatar, "due": due }))
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    })();
    match tiles {
        Ok(list) => Json(list).into_response(),
        Err(e) => internal(e),
    }
}

async fn child_home(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    let conn = s.db.lock().unwrap();
    let home = today(&conn).and_then(|t| match t {
        Some(t) => study::home(&conn, id, t),
        None => Ok(None),
    });
    match home {
        Ok(Some(h)) => Json(h).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct SessionQuery {
    subject: Option<i64>,
}

async fn child_session(
    State(s): State<Shared>,
    Path(id): Path<i64>,
    Query(q): Query<SessionQuery>,
) -> Response {
    let conn = s.db.lock().unwrap();
    let cards = today(&conn).and_then(|t| match t {
        Some(t) => study::session(&conn, id, q.subject, t, &mut rand::rng()),
        None => Ok(vec![]),
    });
    match cards {
        Ok(cards) => Json(cards).into_response(),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct AnswerBody {
    id: String,
    child_id: i64,
    card_id: i64,
    direction: u8,
    known: bool,
}

/// Saves a scheduling answer (spec §5.3). Repeats and retries are harmless: the server
/// decides what counts, so every outcome but "not visible" is a plain success.
async fn answer(State(s): State<Shared>, Json(b): Json<AnswerBody>) -> Response {
    if !(1..=64).contains(&b.id.len()) || b.direction > 1 {
        return error(StatusCode::BAD_REQUEST, "invalid");
    }
    let mut conn = s.db.lock().unwrap();
    let today = match today(&conn) {
        Ok(Some(t)) => t,
        Ok(None) => return error(StatusCode::CONFLICT, "not_set_up"),
        Err(e) => return internal(e),
    };
    let a = Answer {
        id: b.id,
        child_id: b.child_id,
        card_id: b.card_id,
        direction: b.direction,
        known: b.known,
    };
    match store::record_answer(&mut conn, &a, Utc::now(), today) {
        Ok(AnswerOutcome::NotFound) => error(StatusCode::NOT_FOUND, "not_found"),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

async fn image(State(s): State<Shared>, Path(id): Path<i64>) -> Response {
    match study::image(&s.db.lock().unwrap(), id) {
        Ok(Some((mime, data))) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            ],
            data,
        )
            .into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => internal(e),
    }
}
```

`today` is computed per request from the family timezone, so an answer given just after midnight counts for the new day (Plan 1, Review Focus 2). All four routes sit outside the guarded `parent` router on purpose; the host check from the hardening still applies to them.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all pass, 8 more in `api`; fmt and clippy clean. (`web_app_is_served_for_non_api_paths` needs `npm --prefix web run build` first, as before.)

- [ ] **Step 5: Commit**

```bash
git add src/server.rs tests/api.rs
git commit -m "feat: child API for home, study session, answers and images"
```

---

### Task 3: Study queue in the browser

**Files:**
- Create: `web/src/lib/session.ts`, `web/src/lib/session.test.ts`
- Modify: `web/src/lib/text.ts`, `web/src/lib/text.test.ts`

**Interfaces:**
- Produces: `Side`, `StudyCard`, `Turn { card, answerId: string | null }`, `Summary { known, notKnown, repeats }`, `Queue { turns, index, summary }`; `start(cards, id?) -> Queue`; `answer(q, known) -> Queue` (returns a new object, so Svelte's `$state.raw` notices); `newId() -> string`; `whenNext(date: string, today?: Date) -> string`

- [ ] **Step 1: Write the failing tests**

Create `web/src/lib/session.test.ts`:
```ts
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { answer, newId, start, type StudyCard } from './session.ts'

function card(card_id: number, repeat_missed = true): StudyCard {
  const side = { text: `kaart ${card_id}`, image: null }
  return { card_id, direction: 0, subject: 'Frans', deck: 'H1', question: side, answer: side, notes: '', repeat_missed }
}

let n = 0
const ids = () => `id${++n}`

test('first showings carry an answer id', () => {
  const q = start([card(1), card(2)], ids)
  assert.equal(q.turns.length, 2)
  assert.ok(q.turns.every((t) => t.answerId !== null))
  assert.notEqual(q.turns[0].answerId, q.turns[1].answerId)
})

test('known cards leave the queue and are counted', () => {
  let q = start([card(1), card(2)], ids)
  q = answer(q, true)
  q = answer(q, true)
  assert.equal(q.index, 2)
  assert.equal(q.turns.length, 2)
  assert.deepEqual(q.summary, { known: 2, notKnown: 0, repeats: 0 })
})

test('missed cards return at the end until known once', () => {
  let q = start([card(1), card(2)], ids)
  q = answer(q, false) // card 1 missed
  assert.equal(q.turns.length, 3)
  assert.equal(q.turns[2].card.card_id, 1)
  assert.equal(q.turns[2].answerId, null) // practice: not sent to the server
  q = answer(q, true) // card 2
  q = answer(q, false) // card 1 repeat, missed again
  assert.equal(q.turns.length, 4)
  q = answer(q, true) // card 1 repeat, known
  assert.equal(q.index, q.turns.length)
  assert.deepEqual(q.summary, { known: 1, notKnown: 1, repeats: 2 })
})

test('missed cards leave when the study system does not repeat them', () => {
  let q = start([card(1, false)], ids)
  q = answer(q, false)
  assert.equal(q.turns.length, 1)
  assert.deepEqual(q.summary, { known: 0, notKnown: 1, repeats: 0 })
})

test('answering past the end changes nothing', () => {
  const q = start([], ids)
  assert.equal(answer(q, true), q)
})

test('new ids are 32 hex characters and differ', () => {
  const a = newId()
  assert.match(a, /^[0-9a-f]{32}$/)
  assert.notEqual(a, newId())
})
```

`web/src/lib/text.test.ts` becomes:
```ts
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { initial, whenNext } from './text.ts'

test('initial is the first letter', () => {
  assert.equal(initial('Lotte'), 'L')
})

test('initial keeps an emoji whole', () => {
  assert.equal(initial('🦊 Lotte'), '🦊')
  assert.equal(initial('👩‍👧 Mama'), '👩‍👧')
  assert.equal(initial('🇧🇪 Wout'), '🇧🇪')
})

test('initial of an empty name is empty', () => {
  assert.equal(initial(''), '')
})

test('next cards tomorrow say "morgen"', () => {
  assert.equal(whenNext('2026-10-19', new Date(2026, 9, 18, 21, 30)), 'morgen')
})

test('later next cards name the day', () => {
  assert.equal(whenNext('2026-10-24', new Date(2026, 9, 18)), 'op zaterdag 24 oktober')
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix web test`
Expected: FAIL, `Cannot find module …/session.ts` and `whenNext` is not exported.

- [ ] **Step 3: Implement**

Create `web/src/lib/session.ts`:
```ts
/** A card side as the server sends it. */
export type Side = { text: string; image: number | null }

/** One due card direction, turned so `question` is shown first. */
export type StudyCard = {
  card_id: number
  direction: number
  subject: string
  deck: string
  question: Side
  answer: Side
  notes: string
  repeat_missed: boolean
}

/** One showing of a card. First showings carry the id the server records the answer
 *  under; repeats of missed cards are practice (spec §5.4) and are never sent. */
export type Turn = { card: StudyCard; answerId: string | null }

export type Summary = { known: number; notKnown: number; repeats: number }

export type Queue = { turns: Turn[]; index: number; summary: Summary }

/** Random hex id. `crypto.randomUUID` needs a secure context, which plain http on the
 *  home network is not; `getRandomValues` works everywhere. */
export function newId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

export function start(cards: StudyCard[], id: () => string = newId): Queue {
  return {
    turns: cards.map((card) => ({ card, answerId: id() })),
    index: 0,
    summary: { known: 0, notKnown: 0, repeats: 0 },
  }
}

/** The queue after answering the current turn. A missed card goes to the end of the
 *  queue when its study system repeats missed cards, until it is known once. */
export function answer(q: Queue, known: boolean): Queue {
  const turn = q.turns[q.index]
  if (!turn) return q
  const s = { ...q.summary }
  if (turn.answerId === null) s.repeats += 1
  else if (known) s.known += 1
  else s.notKnown += 1
  const repeat = !known && turn.card.repeat_missed
  return {
    turns: repeat ? [...q.turns, { card: turn.card, answerId: null }] : q.turns,
    index: q.index + 1,
    summary: s,
  }
}
```

`web/src/lib/text.ts` becomes:
```ts
/** First visible character of a name, keeping emoji and flags whole. */
export function initial(name: string): string {
  const first = new Intl.Segmenter().segment(name)[Symbol.iterator]().next()
  return first.done ? '' : first.value.segment
}

/** When the next cards come up, as the end of "De volgende kaartjes komen …". */
export function whenNext(date: string, today: Date = new Date()): string {
  const [y, m, d] = date.split('-').map(Number)
  const next = new Date(y, m - 1, d)
  const tomorrow = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 1)
  if (next.getTime() === tomorrow.getTime()) return 'morgen'
  return `op ${next.toLocaleDateString('nl-BE', { weekday: 'long', day: 'numeric', month: 'long' })}`
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm --prefix web test && npm --prefix web run check`
Expected: 11 web tests pass (6 new session, 2 new text); svelte-check 0 errors, 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib
git commit -m "feat: study queue with missed-card repeats"
```

---

### Task 4: Child screens

**Files:**
- Create: `web/src/routes/ChildHome.svelte`, `web/src/routes/Study.svelte`
- Modify: `web/src/routes/Profiles.svelte`, `web/src/App.svelte`, `web/src/app.css`

**Interfaces:**
- Consumes: Task 2 routes; Task 3 `start`, `answer`, `whenNext`, `initial`

- [ ] **Step 1: Child home**

Create `web/src/routes/ChildHome.svelte`:
```svelte
<script lang="ts">
  import { api } from '../lib/api'
  import { initial, whenNext } from '../lib/text'

  type Home = {
    name: string
    avatar: string
    due: number
    subjects: { id: number; name: string; due: number }[]
    answered_today: number
    next_due: string | null
  }

  let { childId }: { childId: number } = $props()

  const home = $derived(api<Home>(`children/${childId}/home`))
</script>

{#await home}
  <p class="notice">Laden…</p>
{:then h}
  <header class="bar">
    <h1 class="greeting">
      <span class="avatar small" style:background={h.avatar}>{initial(h.name)}</span>
      Hoi {h.name}!
    </h1>
    <a class="button secondary" href="#/">Iemand anders</a>
  </header>

  {#if h.due === 0}
    <div class="card cheer">
      <h2>Klaar voor vandaag! 🎉</h2>
      {#if h.next_due}<p>De volgende kaartjes komen {whenNext(h.next_due)}.</p>{/if}
    </div>
  {:else}
    {@const total = h.answered_today + h.due}
    <div class="card today">
      <p class="due"><strong>{h.due}</strong> {h.due === 1 ? 'kaartje' : 'kaartjes'} om te oefenen</p>
      <div
        class="progress"
        role="progressbar"
        aria-label="Vandaag al geoefend"
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={h.answered_today}
      >
        <span style:width="{(100 * h.answered_today) / total}%"></span>
      </div>
      <a class="button big" href="#/kind/{childId}/oefenen">Start met oefenen</a>
    </div>

    <h2>Of kies een vak</h2>
    <ul class="subjects">
      {#each h.subjects as subject (subject.id)}
        <li>
          <a class="subject" href="#/kind/{childId}/oefenen/{subject.id}">
            <span>{subject.name}</span>
            <span class="badge">{subject.due}</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
{:catch}
  <div class="card narrow">
    <p>Dit profiel bestaat niet (meer).</p>
    <div class="actions"><a class="button" href="#/">Terug</a></div>
  </div>
{/await}
```

- [ ] **Step 2: Study and summary**

Create `web/src/routes/Study.svelte`:
```svelte
<script lang="ts">
  import { api } from '../lib/api'
  import { answer, start, type Queue, type Side, type StudyCard } from '../lib/session'

  let { childId, subject }: { childId: number; subject: number | null } = $props()

  let queue = $state.raw<Queue | null>(null)
  let loadFailed = $state(false)
  let revealed = $state(false)
  let saving = $state(false)
  /** The answer that failed to save; the card stays until it is saved (spec §6.3). */
  let unsaved = $state<boolean | null>(null)
  let stopped = $state(false)

  $effect(() => {
    const query = subject === null ? '' : `?subject=${subject}`
    api<StudyCard[]>(`children/${childId}/session${query}`).then(
      (cards) => (queue = start(cards)),
      () => (loadFailed = true),
    )
  })

  const turn = $derived(queue?.turns[queue.index])
  const finished = $derived(queue !== null && (stopped || turn === undefined))

  async function respond(known: boolean) {
    if (!queue || !turn || !revealed || saving) return
    if (turn.answerId !== null) {
      saving = true
      try {
        await api('answers', {
          id: turn.answerId,
          child_id: childId,
          card_id: turn.card.card_id,
          direction: turn.card.direction,
          known,
        })
      } catch {
        unsaved = known
        return
      } finally {
        saving = false
      }
    }
    unsaved = null
    revealed = false
    queue = answer(queue, known)
  }

  function onkeydown(e: KeyboardEvent) {
    if (finished || !turn) return
    if (e.key === ' ') {
      // Never let space also "click" a focused answer button.
      e.preventDefault()
      if (!e.repeat) revealed = true
    } else if (!e.repeat && revealed && unsaved === null) {
      if (e.key === 'ArrowLeft') respond(false)
      if (e.key === 'ArrowRight') respond(true)
    }
  }
</script>

<svelte:window {onkeydown} />

{#snippet side(s: Side)}
  {#if s.image !== null}<img class="card-image" src="/api/images/{s.image}" alt="" />{/if}
  {#if s.text}<p class="card-text">{s.text}</p>{/if}
{/snippet}

{#if loadFailed}
  <div class="card narrow">
    <p>De kaartjes konden niet geladen worden.</p>
    <div class="actions"><a class="button" href="#/kind/{childId}">Terug</a></div>
  </div>
{:else if !queue}
  <p class="notice">Laden…</p>
{:else if finished}
  {@const s = queue.summary}
  <div class="card narrow summary">
    {#if s.known + s.notKnown === 0}
      <h1>Niets geoefend</h1>
      <p>Er waren geen kaartjes om te oefenen.</p>
    {:else}
      <h1>Goed gedaan!</h1>
      <ul class="tally">
        <li class="yes"><strong>{s.known}</strong> wist je</li>
        <li class="no"><strong>{s.notKnown}</strong> nog niet</li>
        <li><strong>{s.repeats}</strong> keer herhaald</li>
      </ul>
    {/if}
    <div class="actions"><a class="button big" href="#/">Klaar</a></div>
  </div>
{:else if turn}
  <div class="study">
    <header class="study-bar">
      <span>{turn.card.subject} · {turn.card.deck}</span>
      <span>{queue.index + 1} van {queue.turns.length}</span>
    </header>

    <section class="flashcard" aria-live="polite">
      {@render side(turn.card.question)}
      {#if revealed}
        <hr />
        {@render side(turn.card.answer)}
        {#if turn.card.notes}<p class="notes">{turn.card.notes}</p>{/if}
      {/if}
    </section>

    <div class="answer-buttons">
      {#if unsaved !== null}
        <p class="error" role="alert">Dat lukte even niet. Probeer het nog eens.</p>
        <button disabled={saving} onclick={() => respond(unsaved!)}>Opnieuw proberen</button>
      {:else if !revealed}
        <button class="reveal" onclick={() => (revealed = true)}>Toon antwoord</button>
      {:else}
        <button class="no" disabled={saving} onclick={() => respond(false)}>Wist ik niet</button>
        <button class="yes" disabled={saving} onclick={() => respond(true)}>Wist ik</button>
      {/if}
    </div>

    <div class="actions">
      <button class="secondary" disabled={saving || unsaved !== null} onclick={() => (stopped = true)}>
        Stoppen
      </button>
    </div>
  </div>
{/if}
```

How it behaves:
- **Saving.** First showings are saved before the queue moves on. If saving fails, `unsaved` holds the answer and only "Opnieuw proberen" is offered, so the retry resends the same answer under the same id.
- **Keys.** Space always has its default prevented, so it can't "click" a focused answer button. `e.repeat` is ignored, so holding a key can't answer twice.
- **Stopping.** "Stoppen" is disabled while an answer is unsaved; anything already answered is saved, and the rest stays due (spec §5.5).

- [ ] **Step 3: Tiles, routes and styles**

`web/src/routes/Profiles.svelte` becomes:
```svelte
<script lang="ts">
  import { api } from '../lib/api'
  import { initial } from '../lib/text'

  type Child = { id: number; name: string; avatar: string; due: number }

  const children = api<Child[]>('children')
</script>

<header class="bar">
  <h1>Wie gaat er oefenen?</h1>
  <a class="icon-button" href="#/ouder" aria-label="Ouderzone" title="Ouderzone">🔒</a>
</header>

{#await children}
  <p class="notice">Laden…</p>
{:then list}
  <ul class="tiles">
    {#each list as child (child.id)}
      <li>
        <a class="tile" href="#/kind/{child.id}">
          <span class="avatar" style:background={child.avatar}>{initial(child.name)}</span>
          <span class="tile-name">{child.name}</span>
          {#if child.due > 0}
            <span class="badge" aria-label="{child.due} kaartjes te oefenen">{child.due}</span>
          {:else}
            <span class="badge done" aria-label="Klaar voor vandaag">✓</span>
          {/if}
        </a>
      </li>
    {/each}
  </ul>
{:catch}
  <p class="notice">De kinderen konden niet geladen worden.</p>
{/await}
```

`web/src/App.svelte` becomes (`{#key route}` recreates a screen on every route change, so it reloads its data):
```svelte
<script lang="ts">
  import { api } from './lib/api'
  import ChildHome from './routes/ChildHome.svelte'
  import Parent from './routes/Parent.svelte'
  import Profiles from './routes/Profiles.svelte'
  import Setup from './routes/Setup.svelte'
  import Study from './routes/Study.svelte'

  type Status = { setup_done: boolean; lan_url: string }

  let status = $state<Status | null>(null)
  let failed = $state(false)
  let route = $state(location.hash)
  // #/kind/<child>, #/kind/<child>/oefenen, #/kind/<child>/oefenen/<subject>
  const child = $derived(route.match(/^#\/kind\/(\d+)(\/oefenen(?:\/(\d+))?)?$/))

  $effect(() => {
    const onHash = () => (route = location.hash)
    addEventListener('hashchange', onHash)
    return () => removeEventListener('hashchange', onHash)
  })

  async function load() {
    try {
      status = await api<Status>('status')
      failed = false
    } catch {
      failed = true
    }
  }
  load()
</script>

<main>
  {#if failed}
    <div class="card narrow">
      <p>Wistjet is niet bereikbaar. Draait het programma nog?</p>
      <div class="actions"><button onclick={load}>Opnieuw proberen</button></div>
    </div>
  {:else if !status}
    <p class="notice">Laden…</p>
  {:else if !status.setup_done}
    <Setup lanUrl={status.lan_url} onDone={load} />
  {:else if route === '#/ouder'}
    <Parent />
  {:else if child?.[2]}
    {#key route}<Study childId={Number(child[1])} subject={child[3] ? Number(child[3]) : null} />{/key}
  {:else if child}
    {#key route}<ChildHome childId={Number(child[1])} />{/key}
  {:else}
    <Profiles />
  {/if}
</main>
```

In `web/src/app.css`:
- add to `:root` after `--error: #c0392b;`:
```css
  --yes: #2e9d63;
  --no: #d9534f;
```
- in the `.tile` rule, add before `display: flex;`:
```css
  position: relative;
  color: inherit;
  text-decoration: none;
```
- append:
```css
.badge {
  min-width: 32px;
  height: 32px;
  padding: 0 10px;
  border-radius: 999px;
  display: inline-grid;
  place-items: center;
  font-weight: 800;
  background: var(--accent);
  color: var(--accent-ink);
}

.tile .badge {
  position: absolute;
  top: 12px;
  right: 12px;
}

.badge.done {
  background: var(--yes);
}

.greeting {
  display: flex;
  align-items: center;
  gap: 12px;
}

.avatar.small {
  width: 48px;
  height: 48px;
  font-size: 1.4rem;
}

h2 {
  font-size: 1.25rem;
  margin: 28px 0 12px;
}

.today .due {
  font-size: 1.3rem;
  margin: 0 0 16px;
}

.today .due strong {
  font-size: 2.4rem;
}

.progress {
  height: 14px;
  border-radius: 999px;
  background: var(--line);
  overflow: hidden;
  margin-bottom: 24px;
}

.progress span {
  display: block;
  height: 100%;
  background: var(--yes);
}

.big {
  font-size: 1.2rem;
  padding: 16px 32px;
  min-height: 56px;
}

.cheer {
  text-align: center;
}

.cheer h2 {
  margin-top: 0;
  font-size: 1.6rem;
}

.subjects {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 12px;
}

.subject {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 18px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  color: inherit;
  text-decoration: none;
  font-weight: 700;
  font-size: 1.1rem;
}

.study {
  max-width: 640px;
  margin: 0 auto;
}

.study-bar span:last-child,
.bar .button {
  white-space: nowrap;
}

.study-bar {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: var(--muted);
  font-weight: 600;
  margin-bottom: 12px;
}

.flashcard {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  box-shadow: 0 4px 20px rgb(43 42 51 / 8%);
  min-height: 260px;
  padding: 32px 24px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  gap: 12px;
}

.flashcard hr {
  width: 60%;
  border: none;
  border-top: 2px dashed var(--line);
  margin: 8px 0;
}

.card-text {
  font-size: clamp(1.6rem, 5vw, 2.4rem);
  font-weight: 700;
  margin: 0;
  overflow-wrap: anywhere;
}

.card-image {
  max-width: 100%;
  max-height: 220px;
  border-radius: 12px;
}

.notes {
  color: var(--muted);
  margin: 0;
}

.answer-buttons {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 12px;
  margin: 20px 0 8px;
}

.answer-buttons .error {
  grid-column: 1 / -1;
  text-align: center;
  margin: 0;
}

.answer-buttons button {
  font-size: 1.2rem;
  min-height: 64px;
  border-radius: var(--radius);
}

.answer-buttons .no {
  background: var(--no);
}

.answer-buttons .yes {
  background: var(--yes);
}

.summary {
  text-align: center;
}

.tally {
  list-style: none;
  padding: 0;
  margin: 0 0 16px;
  display: grid;
  gap: 8px;
  font-size: 1.2rem;
}

.tally strong {
  font-size: 1.8rem;
  margin-right: 6px;
}

.tally .yes strong {
  color: var(--yes);
}

.tally .no strong {
  color: var(--no);
}

.summary .actions {
  justify-content: center;
}
```

- [ ] **Step 4: Check and build**

Run: `npm --prefix web run check && npm --prefix web test && npm --prefix web run build && cargo test`
Expected: svelte-check 0 errors, 0 warnings; web tests pass; build succeeds; all Rust tests pass.

- [ ] **Step 5: Look at it**

Run `cargo run --release` with a fresh data folder (`WISTJET_DATA_DIR=$(mktemp -d)`), finish setup with the sample deck, then check:
- the tile shows a badge of 10;
- the home shows 10 cards, an empty progress bar and "Frans 10";
- start: question, "1 van 10", "Toon antwoord"; space reveals; → and ← answer;
- answer one card "Wist ik niet": it comes back at the end and the total becomes 11;
- with focus on "Wist ik" (Tab to it), space does not answer;
- stop the server mid-session and answer: "Dat lukte even niet" appears and the card stays; start the server again, press "Opnieuw proberen", and the session continues;
- finish: the summary shows known, not known and repeats; "Klaar" returns to the profiles, where the badge has dropped;
- the home now says "Klaar voor vandaag" with when the next cards come (after answering all 10);
- at a phone width (browser dev tools, 390px) nothing overflows.

- [ ] **Step 6: Commit**

```bash
git add web/src
git commit -m "feat: child home, study and summary screens"
```

---

## Done when

- All Rust and web tests pass (Plan 3 adds 9 library, 8 API and 8 web tests), and the web check and build are clean.
- The Task 4 click-through behaves as listed.
- Spec coverage after this plan: §6.1 badges, §6.2 child home, §6.3 study (keyboard, touch, retry), §6.4 summary, §5.4 repeat missed cards (client side), §5.5 session order and stopping. Plan 4 adds the parent screens that create cards, images and study systems.
