//! What a child sees (spec §6): home counts, the study queue, and card images.

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
