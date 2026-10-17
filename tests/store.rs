use chrono::{DateTime, Days, NaiveDate, Utc};
use rusqlite::Connection;
use wistjet::db;
use wistjet::engine::Progress;
use wistjet::store::{self, Answer, AnswerOutcome};

const LOTTE: i64 = 1;
const WOUT: i64 = 2;
const DECK: i64 = 1;
const CHAT: i64 = 1;
const CHIEN: i64 = 2;

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn now() -> DateTime<Utc> {
    "2026-10-10T08:00:00Z".parse().unwrap()
}

/// Two children on one French deck (front → back) with two cards; a second, empty subject.
fn fixture() -> Connection {
    let conn = db::open_in_memory().unwrap();
    conn.execute_batch(
        "INSERT INTO child (id, name, avatar) VALUES (1, 'Lotte', '#e57373'), (2, 'Wout', '#64b5f6');
         INSERT INTO subject (id, name) VALUES (1, 'Frans'), (2, 'Wiskunde');
         INSERT INTO deck (id, subject_id, name, system_id, direction_mode)
           VALUES (1, 1, 'Woordjes H1', 1, 'front_to_back');
         INSERT INTO deck_child (deck_id, child_id) VALUES (1, 1), (1, 2);
         INSERT INTO card (id, deck_id, front_text, back_text)
           VALUES (1, 1, 'le chat', 'de kat'), (2, 1, 'le chien', 'de hond');",
    )
    .unwrap();
    conn
}

fn answer(
    conn: &mut Connection,
    id: &str,
    child: i64,
    card: i64,
    dir: u8,
    known: bool,
    today: &str,
) -> AnswerOutcome {
    let a = Answer {
        id: id.into(),
        child_id: child,
        card_id: card,
        direction: dir,
        known,
    };
    store::record_answer(conn, &a, now(), d(today)).unwrap()
}

fn due(conn: &Connection, child: i64, today: &str) -> Vec<(i64, u8)> {
    let mut v: Vec<(i64, u8)> = store::due_items(conn, child, None, d(today))
        .unwrap()
        .iter()
        .map(|i| (i.card_id, i.direction))
        .collect();
    v.sort();
    v
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn new_cards_are_due_today() {
    let conn = fixture();
    assert_eq!(due(&conn, LOTTE, "2026-10-10"), vec![(CHAT, 0), (CHIEN, 0)]);
}

#[test]
fn back_to_front_deck_uses_direction_one() {
    let conn = fixture();
    conn.execute_batch("UPDATE deck SET direction_mode = 'back_to_front'")
        .unwrap();
    assert_eq!(due(&conn, LOTTE, "2026-10-10"), vec![(CHAT, 1), (CHIEN, 1)]);
}

#[test]
fn both_mode_lists_both_directions() {
    let conn = fixture();
    conn.execute_batch("UPDATE deck SET direction_mode = 'both'")
        .unwrap();
    assert_eq!(
        due(&conn, LOTTE, "2026-10-10"),
        vec![(CHAT, 0), (CHAT, 1), (CHIEN, 0), (CHIEN, 1)]
    );
}

#[test]
fn subject_filter() {
    let conn = fixture();
    assert_eq!(
        store::due_items(&conn, LOTTE, Some(1), d("2026-10-10"))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        store::due_items(&conn, LOTTE, Some(2), d("2026-10-10"))
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn known_answer_schedules_and_leaves_today() {
    let mut conn = fixture();
    let out = answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10");
    assert_eq!(
        out,
        AnswerOutcome::Scheduled(Progress {
            box_pos: 1,
            due: d("2026-10-12")
        })
    );
    assert_eq!(due(&conn, LOTTE, "2026-10-10"), vec![(CHIEN, 0)]);
    assert_eq!(due(&conn, LOTTE, "2026-10-11"), vec![(CHIEN, 0)]);
    assert_eq!(due(&conn, LOTTE, "2026-10-12"), vec![(CHAT, 0), (CHIEN, 0)]);
    assert_eq!(count(&conn, "SELECT count(*) FROM review_log"), 1);
}

#[test]
fn duplicate_submission_counts_once() {
    let mut conn = fixture();
    answer(&mut conn, "a1", LOTTE, CHAT, 0, false, "2026-10-10");
    let again = answer(&mut conn, "a1", LOTTE, CHAT, 0, false, "2026-10-10");
    assert_eq!(again, AnswerOutcome::AlreadyRecorded);
    assert_eq!(count(&conn, "SELECT count(*) FROM review_log"), 1);
    assert_eq!(count(&conn, "SELECT incorrect_count FROM progress"), 1);
}

#[test]
fn later_answer_the_same_day_is_practice() {
    let mut conn = fixture();
    answer(&mut conn, "a1", LOTTE, CHAT, 0, false, "2026-10-10");
    let repeat = answer(&mut conn, "a2", LOTTE, CHAT, 0, true, "2026-10-10");
    assert_eq!(repeat, AnswerOutcome::Practice);
    assert_eq!(count(&conn, "SELECT box_position FROM progress"), 0);
    assert_eq!(count(&conn, "SELECT count(*) FROM review_log"), 1);
}

#[test]
fn overdue_cards_accumulate_and_reschedule_from_answer_day() {
    let mut conn = fixture();
    answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10"); // box 2, due 12th
    assert_eq!(due(&conn, LOTTE, "2026-10-20"), vec![(CHAT, 0), (CHIEN, 0)]);
    let out = answer(&mut conn, "a2", LOTTE, CHAT, 0, true, "2026-10-20");
    assert_eq!(
        out,
        AnswerOutcome::Scheduled(Progress {
            box_pos: 2,
            due: d("2026-10-27")
        })
    );
}

#[test]
fn progress_is_independent_per_child() {
    let mut conn = fixture();
    answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10");
    assert_eq!(due(&conn, WOUT, "2026-10-10"), vec![(CHAT, 0), (CHIEN, 0)]);
}

#[test]
fn progress_is_independent_per_direction() {
    let mut conn = fixture();
    conn.execute_batch("UPDATE deck SET direction_mode = 'both'")
        .unwrap();
    answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10");
    assert_eq!(
        due(&conn, LOTTE, "2026-10-10"),
        vec![(CHAT, 1), (CHIEN, 0), (CHIEN, 1)]
    );
}

#[test]
fn hiding_keeps_progress_and_restoring_resumes_it() {
    let hide_restore = [
        (
            "UPDATE card SET active = 0 WHERE id = 1",
            "UPDATE card SET active = 1 WHERE id = 1",
        ),
        (
            "UPDATE deck SET archived = 1",
            "UPDATE deck SET archived = 0",
        ),
        (
            "UPDATE subject SET archived = 1 WHERE id = 1",
            "UPDATE subject SET archived = 0 WHERE id = 1",
        ),
        (
            "UPDATE child SET archived = 1 WHERE id = 1",
            "UPDATE child SET archived = 0 WHERE id = 1",
        ),
        (
            "DELETE FROM deck_child WHERE child_id = 1",
            "INSERT INTO deck_child VALUES (1, 1)",
        ),
        (
            "UPDATE deck SET direction_mode = 'back_to_front'",
            "UPDATE deck SET direction_mode = 'front_to_back'",
        ),
    ];
    for (hide, restore) in hide_restore {
        let mut conn = fixture();
        answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10"); // box 2, due 12th
        conn.execute_batch(hide).unwrap();
        assert!(
            !due(&conn, LOTTE, "2026-10-12").contains(&(CHAT, 0)),
            "{hide}"
        );
        conn.execute_batch(restore).unwrap();
        assert!(
            due(&conn, LOTTE, "2026-10-12").contains(&(CHAT, 0)),
            "{restore}"
        );
        let out = answer(&mut conn, "a2", LOTTE, CHAT, 0, true, "2026-10-12");
        assert_eq!(
            out,
            AnswerOutcome::Scheduled(Progress {
                box_pos: 2,
                due: d("2026-10-19")
            }),
            "{hide}"
        );
    }
}

#[test]
fn hidden_card_cannot_be_answered() {
    let mut conn = fixture();
    conn.execute_batch("UPDATE card SET active = 0 WHERE id = 1")
        .unwrap();
    assert_eq!(
        answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10"),
        AnswerOutcome::NotFound
    );
    assert_eq!(
        answer(&mut conn, "a2", LOTTE, CHIEN, 1, true, "2026-10-10"),
        AnswerOutcome::NotFound
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM review_log"), 0);
}

#[test]
fn full_cycle_through_the_school_boxes() {
    let mut conn = fixture();
    let mut today = d("2026-10-10");
    let expected = [(1, 2), (2, 7), (3, 14), (3, 14)];
    for (n, (box_pos, interval)) in expected.into_iter().enumerate() {
        let a = Answer {
            id: format!("a{n}"),
            child_id: LOTTE,
            card_id: CHAT,
            direction: 0,
            known: true,
        };
        let out = store::record_answer(&mut conn, &a, now(), today).unwrap();
        let next = today + Days::new(interval);
        assert_eq!(
            out,
            AnswerOutcome::Scheduled(Progress { box_pos, due: next })
        );
        today = next;
    }
}

#[test]
fn reset_one_child_on_one_deck() {
    let mut conn = fixture();
    answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10");
    answer(&mut conn, "a2", WOUT, CHAT, 0, true, "2026-10-10");
    store::reset_progress(&conn, DECK, Some(LOTTE), d("2026-10-11")).unwrap();
    assert_eq!(due(&conn, LOTTE, "2026-10-11"), vec![(CHAT, 0), (CHIEN, 0)]);
    assert_eq!(due(&conn, WOUT, "2026-10-11"), vec![(CHIEN, 0)]);
    assert_eq!(
        count(
            &conn,
            "SELECT box_position FROM progress WHERE child_id = 1"
        ),
        0
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM review_log"), 2);
}

#[test]
fn reset_whole_deck() {
    let mut conn = fixture();
    answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10");
    answer(&mut conn, "a2", WOUT, CHAT, 0, true, "2026-10-10");
    store::reset_progress(&conn, DECK, None, d("2026-10-11")).unwrap();
    assert_eq!(due(&conn, LOTTE, "2026-10-11"), vec![(CHAT, 0), (CHIEN, 0)]);
    assert_eq!(due(&conn, WOUT, "2026-10-11"), vec![(CHAT, 0), (CHIEN, 0)]);
}

#[test]
fn answer_after_same_day_reset_is_scheduling() {
    let mut conn = fixture();
    answer(&mut conn, "a1", LOTTE, CHAT, 0, true, "2026-10-10");
    store::reset_progress(&conn, DECK, Some(LOTTE), d("2026-10-10")).unwrap();
    let out = answer(&mut conn, "a2", LOTTE, CHAT, 0, false, "2026-10-10");
    assert_eq!(
        out,
        AnswerOutcome::Scheduled(Progress {
            box_pos: 0,
            due: d("2026-10-11")
        })
    );
}
