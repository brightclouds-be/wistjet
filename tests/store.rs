use chrono::NaiveDate;
use rusqlite::Connection;
use wistjet::db;
use wistjet::store;

const LOTTE: i64 = 1;
const CHAT: i64 = 1;
const CHIEN: i64 = 2;

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
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

fn due(conn: &Connection, child: i64, today: &str) -> Vec<(i64, u8)> {
    let mut v: Vec<(i64, u8)> = store::due_items(conn, child, None, d(today))
        .unwrap()
        .iter()
        .map(|i| (i.card_id, i.direction))
        .collect();
    v.sort();
    v
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
