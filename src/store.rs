//! Queries for due cards, answers and resets. Rules live in `engine`; this file only loads and saves.

use crate::engine::DueItem;
use chrono::NaiveDate;
use rusqlite::{Connection, Result, named_params};

/// Card directions a child may currently study (spec §5.2 minus the due-date check).
/// Hidden progress (§5.8) is simply not joined; it is never deleted.
const VISIBLE: &str = "
    FROM deck_child dc
    JOIN child ch ON ch.id = dc.child_id AND ch.archived = 0
    JOIN deck d ON d.id = dc.deck_id AND d.archived = 0
    JOIN subject s ON s.id = d.subject_id AND s.archived = 0
    JOIN card c ON c.deck_id = d.id AND c.active = 1
    JOIN (SELECT 0 AS dir UNION ALL SELECT 1) x
      ON d.direction_mode = 'both'
      OR (d.direction_mode = 'front_to_back' AND x.dir = 0)
      OR (d.direction_mode = 'back_to_front' AND x.dir = 1)
    LEFT JOIN progress p
      ON p.child_id = dc.child_id AND p.card_id = c.id AND p.direction = x.dir
    WHERE dc.child_id = :child";

/// Due card directions for a child, optionally for one subject. Unordered; see `engine::order_session`.
pub fn due_items(
    conn: &Connection,
    child_id: i64,
    subject_id: Option<i64>,
    today: NaiveDate,
) -> Result<Vec<DueItem>> {
    let sql = format!(
        "SELECT c.id, x.dir, COALESCE(p.due_date, :today) {VISIBLE}
           AND (:subject IS NULL OR d.subject_id = :subject)
           AND COALESCE(p.due_date, :today) <= :today"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! { ":child": child_id, ":today": today, ":subject": subject_id },
        |r| {
            Ok(DueItem {
                card_id: r.get(0)?,
                direction: r.get(1)?,
                due: r.get(2)?,
            })
        },
    )?;
    rows.collect()
}
