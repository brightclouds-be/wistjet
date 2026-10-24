//! What a child sees (spec §6): home counts, the study queue, and card images.

use crate::engine;
use crate::store::{self, VISIBLE};
use chrono::NaiveDate;
use rand::Rng;
use rusqlite::{Connection, OptionalExtension, Result, named_params, params};
use serde::Serialize;
use std::collections::{HashMap, hash_map::Entry};

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SubjectDue {
    pub id: i64,
    pub name: String,
    pub due: i64,
}

/// How many of a subject's card directions sit in one box, and how many of those are due.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct BoxCount {
    /// How often the box's cards come back; the child sees this, not the box name.
    pub interval_days: i64,
    pub cards: i64,
    pub due: i64,
}

/// A subject's cards spread over the boxes, first box first.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SubjectBoxes {
    pub id: i64,
    pub name: String,
    pub boxes: Vec<BoxCount>,
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
    /// Every subject with cards, due or not, as the child's progress per box.
    pub progress: Vec<SubjectBoxes>,
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
    /// The child's time limit on "Typen op tijd" decks; `None` when the child judges
    /// their own answer (tables spec §7).
    pub time_limit_ms: Option<i64>,
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
    let progress = progress(conn, child_id, today)?;
    Ok(Some(Home {
        name,
        avatar,
        due,
        subjects,
        answered_today,
        next_due,
        progress,
    }))
}

/// Cards per box for each subject. Decks of one subject on different study systems
/// are matched by box position; a box takes its interval from the first deck that has it.
fn progress(conn: &Connection, child_id: i64, today: NaiveDate) -> Result<Vec<SubjectBoxes>> {
    let sql = format!(
        "SELECT s.id, s.name, d.system_id, COALESCE(p.box_position, 0), count(*),
                sum(COALESCE(p.due_date, :today) <= :today)
         {VISIBLE} GROUP BY 1, 3, 4 ORDER BY s.name, s.id, 3, 4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(named_params! { ":child": child_id, ":today": today }, |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, usize>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    let mut subjects: Vec<SubjectBoxes> = vec![];
    let mut systems = HashMap::new();
    for row in rows {
        let (id, name, system_id, pos, cards, due) = row?;
        if subjects.last().is_none_or(|s| s.id != id) {
            subjects.push(SubjectBoxes {
                id,
                name,
                boxes: vec![],
            });
        }
        if let Entry::Vacant(e) = systems.entry(system_id) {
            e.insert(store::load_boxes(conn, system_id)?);
        }
        let boxes = &systems[&system_id];
        let subject = subjects.last_mut().unwrap();
        for b in &boxes[subject.boxes.len().min(boxes.len())..] {
            subject.boxes.push(BoxCount {
                interval_days: i64::from(b.interval_days),
                cards: 0,
                due: 0,
            });
        }
        // A stale position past the last box counts as the last box (Plan 1).
        let slot = &mut subject.boxes[pos.min(boxes.len() - 1)];
        slot.cards += cards;
        slot.due += due;
    }
    Ok(subjects)
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
                c.notes, ss.repeat_missed,
                -- A back without text can't be typed: the child judges that card as usual.
                CASE WHEN d.answer_mode = 'timed' AND trim(c.back_text) <> ''
                     THEN (SELECT time_limit_ms FROM child WHERE id = ?2) END
         FROM card c
         JOIN deck d ON d.id = c.deck_id
         JOIN subject s ON s.id = d.subject_id
         JOIN study_system ss ON ss.id = d.system_id
         WHERE c.id = ?1",
    )?;
    items
        .iter()
        .map(|item| {
            stmt.query_row([item.card_id, child_id], |r| {
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
                    time_limit_ms: r.get(8)?,
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
    fn home_shows_each_subjects_cards_per_box() {
        let mut conn = fixture();
        answer(&mut conn, "a1", 1); // le chat: box 1 → box 2
        // A position past the last box (Plan 1) counts as the last box.
        conn.execute_batch(
            "INSERT INTO progress (child_id, card_id, direction, box_position, due_date)
             VALUES (1, 3, 0, 9, '2026-10-17')",
        )
        .unwrap();
        let h = home(&conn, 1, d(TODAY)).unwrap().unwrap();
        let boxes = |n: [(i64, i64); 4]| -> Vec<BoxCount> {
            [1, 2, 7, 14]
                .iter()
                .zip(n)
                .map(|(&interval_days, (cards, due))| BoxCount {
                    interval_days,
                    cards,
                    due,
                })
                .collect()
        };
        assert_eq!(
            h.progress,
            vec![
                SubjectBoxes {
                    id: 1,
                    name: "Frans".into(),
                    boxes: boxes([(1, 1), (1, 0), (0, 0), (0, 0)])
                },
                SubjectBoxes {
                    id: 2,
                    name: "Wiskunde".into(),
                    boxes: boxes([(0, 0), (0, 0), (0, 0), (1, 1)])
                },
            ]
        );
    }

    #[test]
    fn decks_on_other_systems_join_their_subject_by_box_position() {
        let conn = fixture();
        conn.execute_batch(
            "INSERT INTO study_system (id, name) VALUES (2, 'Drie bakjes');
             INSERT INTO box VALUES (2, 0, 'A', 3, 1, 0), (2, 1, 'B', 5, 2, 0), (2, 2, 'C', 30, 2, 0);
             UPDATE deck SET system_id = 2 WHERE id = 2;
             INSERT INTO deck (id, subject_id, name, system_id, direction_mode)
               VALUES (3, 2, 'Breuken', 1, 'front_to_back');
             INSERT INTO deck_child (deck_id, child_id) VALUES (3, 1);
             INSERT INTO card (id, deck_id, front_text, back_text, notes) VALUES (4, 3, '1/2', '0,5', '');
             INSERT INTO progress (child_id, card_id, direction, box_position, due_date)
               VALUES (1, 4, 0, 3, '2026-10-30');",
        )
        .unwrap();
        let h = home(&conn, 1, d(TODAY)).unwrap().unwrap();
        let maths = &h.progress[1].boxes;
        let got: Vec<_> = maths.iter().map(|b| (b.interval_days, b.cards)).collect();
        // School 4-box (system 1) is read first; the 3-box deck's card sits in box 1.
        assert_eq!(got, vec![(1, 1), (2, 0), (7, 0), (14, 1)]);
    }

    #[test]
    fn progress_stays_when_nothing_is_due() {
        let mut conn = fixture();
        for card in 1..=3 {
            answer(&mut conn, &format!("a{card}"), card);
        }
        let h = home(&conn, 1, d(TODAY)).unwrap().unwrap();
        assert_eq!(h.progress.len(), 2);
        assert_eq!(h.progress[0].boxes[1].cards, 2);
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
                time_limit_ms: None,
            }]
        );
        assert_eq!(session_for(&conn, None).len(), 3);
    }

    #[test]
    fn timed_decks_send_the_childs_time_limit() {
        let conn = fixture();
        conn.execute_batch(
            "UPDATE deck SET answer_mode = 'timed' WHERE id = 2;
             UPDATE child SET time_limit_ms = 4500 WHERE id = 1;",
        )
        .unwrap();
        let limits: Vec<_> = session_for(&conn, None)
            .iter()
            .map(|c| (c.card_id, c.time_limit_ms))
            .collect();
        assert!(limits.contains(&(3, Some(4500))));
        assert!(limits.contains(&(1, None)));
        assert!(limits.contains(&(2, None)));
    }

    #[test]
    fn timed_cards_without_answer_text_are_judged_by_the_child() {
        let conn = fixture();
        conn.execute_batch(
            "UPDATE deck SET answer_mode = 'timed' WHERE id = 2;
             INSERT INTO image (id, mime, data) VALUES (1, 'image/png', x'89504E47');
             UPDATE card SET back_text = '  ', back_image_id = 1 WHERE id = 3;",
        )
        .unwrap();
        assert_eq!(session_for(&conn, Some(2))[0].time_limit_ms, None);
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
