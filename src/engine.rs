//! Pure scheduling rules (spec §5). No database, no clock: callers pass `today`.

use chrono::{DateTime, Days, NaiveDate, Utc};
use chrono_tz::Tz;

/// One box of a study system. `success` and `failure` are box positions (0-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxRule {
    pub interval_days: u32,
    pub success: usize,
    pub failure: usize,
}

/// Where a (child, card, direction) stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub box_pos: usize,
    pub due: NaiveDate,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SystemError {
    NoBoxes,
    BadInterval { box_pos: usize },
    BadTarget { box_pos: usize },
}

/// The built-in "School 4-box" preset.
pub fn school_four_box() -> Vec<BoxRule> {
    vec![
        BoxRule {
            interval_days: 1,
            success: 1,
            failure: 0,
        },
        BoxRule {
            interval_days: 2,
            success: 2,
            failure: 0,
        },
        BoxRule {
            interval_days: 7,
            success: 3,
            failure: 0,
        },
        BoxRule {
            interval_days: 14,
            success: 3,
            failure: 0,
        },
    ]
}

/// The family's calendar day at instant `now` (spec §5: days are local calendar days).
pub fn local_today(tz: Tz, now: DateTime<Utc>) -> NaiveDate {
    now.with_timezone(&tz).date_naive()
}

/// A card direction never reviewed starts in the first box, due today (§5.1).
pub fn new_progress(today: NaiveDate) -> Progress {
    Progress {
        box_pos: 0,
        due: today,
    }
}

pub fn is_due(p: Progress, today: NaiveDate) -> bool {
    p.due <= today
}

/// A scheduling answer (§5.3). `current` is clamped so a stale position can't panic.
pub fn schedule(boxes: &[BoxRule], current: usize, known: bool, today: NaiveDate) -> Progress {
    let rule = boxes[current.min(boxes.len() - 1)];
    let box_pos = if known { rule.success } else { rule.failure };
    let interval = boxes[box_pos].interval_days;
    Progress {
        box_pos,
        due: today + Days::new(interval.into()),
    }
}

/// Rules a study system must satisfy before it is saved (§5.6, §8).
pub fn validate_system(boxes: &[BoxRule]) -> Result<(), SystemError> {
    if boxes.is_empty() {
        return Err(SystemError::NoBoxes);
    }
    for (box_pos, b) in boxes.iter().enumerate() {
        if !(1..=365).contains(&b.interval_days) {
            return Err(SystemError::BadInterval { box_pos });
        }
        if b.success >= boxes.len() || b.failure >= boxes.len() {
            return Err(SystemError::BadTarget { box_pos });
        }
    }
    Ok(())
}

/// Maps every old box position to a new one (§5.6).
/// `origins[i]` is the old position the new box `i` came from (`None` = new box).
/// Kept boxes follow their box; a removed box's progress goes to the box now at the
/// same position, or the last box. Switching systems = all origins `None`.
pub fn remap_positions(old_len: usize, origins: &[Option<usize>]) -> Vec<usize> {
    let last = origins.len() - 1;
    (0..old_len)
        .map(|old| {
            origins
                .iter()
                .position(|o| *o == Some(old))
                .unwrap_or(old.min(last))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn known_moves_up_and_sets_due_from_destination_interval() {
        let b = school_four_box();
        let today = d("2026-10-10");
        assert_eq!(
            schedule(&b, 0, true, today),
            Progress {
                box_pos: 1,
                due: d("2026-10-12")
            }
        );
        assert_eq!(
            schedule(&b, 1, true, today),
            Progress {
                box_pos: 2,
                due: d("2026-10-17")
            }
        );
        assert_eq!(
            schedule(&b, 2, true, today),
            Progress {
                box_pos: 3,
                due: d("2026-10-24")
            }
        );
        assert_eq!(
            schedule(&b, 3, true, today),
            Progress {
                box_pos: 3,
                due: d("2026-10-24")
            }
        );
    }

    #[test]
    fn not_known_goes_back_to_first_box_due_tomorrow() {
        let b = school_four_box();
        for from in 0..4 {
            assert_eq!(
                schedule(&b, from, false, d("2026-10-10")),
                Progress {
                    box_pos: 0,
                    due: d("2026-10-11")
                }
            );
        }
    }

    #[test]
    fn new_card_is_in_first_box_and_due_today() {
        let today = d("2026-10-10");
        let p = new_progress(today);
        assert_eq!(p.box_pos, 0);
        assert!(is_due(p, today));
    }

    #[test]
    fn scheduled_card_is_never_due_again_the_same_day() {
        let b = school_four_box();
        let today = d("2026-10-10");
        for from in 0..4 {
            for known in [true, false] {
                assert!(!is_due(schedule(&b, from, known, today), today));
            }
        }
    }

    #[test]
    fn overdue_stays_due_and_reschedules_from_answer_day() {
        let b = school_four_box();
        let p = Progress {
            box_pos: 1,
            due: d("2026-10-01"),
        };
        let today = d("2026-10-10"); // nine days skipped
        assert!(is_due(p, today));
        assert_eq!(schedule(&b, p.box_pos, true, today).due, d("2026-10-17"));
    }

    #[test]
    fn stale_box_position_is_clamped_to_last_box() {
        let b = school_four_box();
        assert_eq!(schedule(&b, 9, true, d("2026-10-10")).box_pos, 3);
    }

    #[test]
    fn local_today_near_midnight_in_brussels() {
        let tz: Tz = "Europe/Brussels".parse().unwrap();
        // CEST = UTC+2
        assert_eq!(
            local_today(tz, utc("2026-10-10T21:59:59Z")),
            d("2026-10-10")
        );
        assert_eq!(
            local_today(tz, utc("2026-10-10T22:00:00Z")),
            d("2026-10-11")
        );
    }

    #[test]
    fn local_today_across_dst_transitions() {
        let tz: Tz = "Europe/Brussels".parse().unwrap();
        // Spring forward 2026-03-29 01:00 UTC; before it Brussels is UTC+1.
        assert_eq!(
            local_today(tz, utc("2026-03-28T22:59:59Z")),
            d("2026-03-28")
        );
        assert_eq!(
            local_today(tz, utc("2026-03-28T23:00:00Z")),
            d("2026-03-29")
        );
        // Fall back 2026-10-25 01:00 UTC; before it Brussels is UTC+2.
        assert_eq!(
            local_today(tz, utc("2026-10-24T21:59:59Z")),
            d("2026-10-24")
        );
        assert_eq!(
            local_today(tz, utc("2026-10-24T22:00:00Z")),
            d("2026-10-25")
        );
        assert_eq!(
            local_today(tz, utc("2026-10-25T22:59:59Z")),
            d("2026-10-25")
        );
    }

    #[test]
    fn intervals_count_calendar_days_across_dst() {
        let b = school_four_box();
        // 7 days from the day before spring-forward is exactly one calendar week.
        assert_eq!(schedule(&b, 1, true, d("2026-03-28")).due, d("2026-04-04"));
    }

    #[test]
    fn validate_system_rules() {
        assert_eq!(validate_system(&[]), Err(SystemError::NoBoxes));
        assert_eq!(validate_system(&school_four_box()), Ok(()));
        let bad_target = [BoxRule {
            interval_days: 1,
            success: 1,
            failure: 0,
        }];
        assert_eq!(
            validate_system(&bad_target),
            Err(SystemError::BadTarget { box_pos: 0 })
        );
        for interval_days in [0, 366] {
            let bad = [BoxRule {
                interval_days,
                success: 0,
                failure: 0,
            }];
            assert_eq!(
                validate_system(&bad),
                Err(SystemError::BadInterval { box_pos: 0 })
            );
        }
    }

    #[test]
    fn remap_when_a_middle_box_is_removed() {
        // Old boxes 0,1,2,3; box 1 removed → new list came from [0, 2, 3].
        let map = remap_positions(4, &[Some(0), Some(2), Some(3)]);
        // Box 1's progress goes to the box now at position 1 (old box 2).
        assert_eq!(map, vec![0, 1, 1, 2]);
    }

    #[test]
    fn remap_when_the_last_box_is_removed() {
        let map = remap_positions(4, &[Some(0), Some(1), Some(2)]);
        assert_eq!(map, vec![0, 1, 2, 2]);
    }

    #[test]
    fn remap_follows_reordered_boxes() {
        let map = remap_positions(2, &[Some(1), Some(0)]);
        assert_eq!(map, vec![1, 0]);
    }

    #[test]
    fn remap_on_system_switch_uses_position_then_last_box() {
        assert_eq!(remap_positions(4, &[None, None]), vec![0, 1, 1, 1]);
        assert_eq!(
            remap_positions(2, &[None, None, None, None, None]),
            vec![0, 1]
        );
    }
}
