# Plan 1: Learning Engine & Storage — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tested Rust library holding every scheduling rule of spec §5, plus the SQLite schema and the queries for due cards, answers and resets.

**Architecture:** Pure functions in `src/engine.rs` (no database, no clock; callers pass `today`). `src/db.rs` opens and migrates the SQLite file. `src/store.rs` loads and saves through the engine. No server or UI yet; those come in Plans 2–5 (see `2026-10-11-wistjet-roadmap.md`).

**Tech Stack:** Rust edition 2024 (stable), `rusqlite` 0.37 (`bundled`, `chrono`), `chrono` 0.4, `chrono-tz` 0.10, `rand` 0.9.

**Spec:** `docs/superpowers/specs/2026-10-08-family-flashcards-design.md`

## Global Constraints

- Rust edition 2024 on the stable toolchain; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must pass.
- No external services or accounts; SQLite file only.
- All "day" calculations use calendar days in the family's timezone; due dates are stored as `YYYY-MM-DD`.
- Every box interval is an integer 1–365; box targets must be boxes within the same system.
- Text limits: names ≤ 50 characters, front/back ≤ 500, notes ≤ 1000. Images: common web formats (JPEG, PNG, WebP, GIF), ≤ 5 MB.
- A card side must have text or an image (or both).
- Hiding (inactive card, archived deck/subject/child, unassignment, narrowing direction) never deletes progress.
- Reset keeps the review log.
- Practice repeats are never logged and never change box or due date.

## Review Focus

Inputs the spec implies but doesn't spell out. Each one has a test in the task named.

1. **Answer after a same-day reset** — a parent resets a deck the child already studied today, and the child studies again. Expected: the answer counts as a scheduling answer (card was put back to "due today"). Test `answer_after_same_day_reset_is_scheduling` (Task 8).
2. **Session started before midnight, answered after** — cards due yesterday are still due today, so the answer schedules from today's date. Test `overdue_cards_accumulate_and_reschedule_from_answer_day` (Task 7).
3. **Stale box position** — progress points past the end of a system that lost boxes. Expected: treated as the last box, no crash. Test `stale_box_position_is_clamped_to_last_box` (Task 1).
4. **Answer for a card the child can't see** — inactive card, wrong direction, unassigned deck. Expected: rejected, nothing written. Test `hidden_card_cannot_be_answered` (Task 7).
5. **Opening a file that isn't ours or is from a newer version** — expected: refused before anything is changed. Tests `rejects_database_from_newer_app`, `rejects_foreign_sqlite_database`, `rejects_file_that_is_not_sqlite` (Task 5).

## File Structure

| File | Responsibility |
|---|---|
| `Cargo.toml` | Crate manifest (library for now; Plan 2 adds the binary) |
| `.gitignore` | Ignore `target/` |
| `.github/workflows/ci.yml` | fmt + clippy + tests on push and PR |
| `src/lib.rs` | Module list |
| `src/engine.rs` | Pure rules: boxes, scheduling, local day, system validation, box remapping, session order. Unit tests inline. |
| `migrations/001_init.sql` | Full MVP schema + School 4-box preset |
| `src/db.rs` | Open, verify, migrate. Unit tests inline. |
| `src/store.rs` | Due query, answer transaction, reset |
| `tests/store.rs` | Store integration tests against in-memory SQLite |

Direction encoding used everywhere: `0` = front → back, `1` = back → front.
Box positions are 0-based in code; the UI shows them 1-based (Plan 3/4).

---

### Task 1: Crate setup and core scheduling

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `.github/workflows/ci.yml`, `src/lib.rs`, `src/engine.rs`

**Interfaces:**
- Produces: `engine::BoxRule { interval_days: u32, success: usize, failure: usize }`, `engine::Progress { box_pos: usize, due: NaiveDate }`, `engine::school_four_box() -> Vec<BoxRule>`, `engine::new_progress(today: NaiveDate) -> Progress`, `engine::is_due(p: Progress, today: NaiveDate) -> bool`, `engine::schedule(boxes: &[BoxRule], current: usize, known: bool, today: NaiveDate) -> Progress`

- [ ] **Step 1: Create the crate files**

`Cargo.toml`:
```toml
[package]
name = "wistjet"
version = "0.1.0"
edition = "2024"

[dependencies]
chrono = "0.4"
chrono-tz = "0.10"
rand = "0.9"
rusqlite = { version = "0.37", features = ["bundled", "chrono"] }
```

`.gitignore`:
```
/target
```

`.github/workflows/ci.yml`:
```yaml
name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test
```

`src/lib.rs`:
```rust
pub mod engine;
```

- [ ] **Step 2: Write the failing tests**

Create `src/engine.rs` with only the test module:
```rust
//! Pure scheduling rules (spec §5). No database, no clock: callers pass `today`.

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
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
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test engine`
Expected: compile error, `cannot find function school_four_box` (and the other items).

- [ ] **Step 4: Implement**

Insert above the `#[cfg(test)]` line in `src/engine.rs`:
```rust
use chrono::{Days, NaiveDate};

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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test engine && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: 6 tests pass; fmt and clippy clean.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore .github src
git commit -m "feat: crate setup and core box scheduling"
```

---

### Task 2: Family-local calendar day

**Files:**
- Modify: `src/engine.rs`

**Interfaces:**
- Consumes: `schedule`, `school_four_box` (Task 1)
- Produces: `engine::local_today(tz: chrono_tz::Tz, now: DateTime<Utc>) -> NaiveDate`

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `src/engine.rs`, add after `fn d`:
```rust
    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }
```
and at the end of the module:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test engine`
Expected: compile error, `cannot find function local_today`.

- [ ] **Step 3: Implement**

Replace the `use chrono::{Days, NaiveDate};` line with:
```rust
use chrono::{DateTime, Days, NaiveDate, Utc};
use chrono_tz::Tz;
```
Add after `school_four_box`:
```rust
/// The family's calendar day at instant `now` (spec §5: days are local calendar days).
pub fn local_today(tz: Tz, now: DateTime<Utc>) -> NaiveDate {
    now.with_timezone(&tz).date_naive()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test engine`
Expected: 9 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/engine.rs
git commit -m "feat: family-local calendar day with DST tests"
```

---

### Task 3: Study system validation and box remapping

**Files:**
- Modify: `src/engine.rs`

**Interfaces:**
- Consumes: `BoxRule`, `school_four_box` (Task 1)
- Produces: `engine::SystemError { NoBoxes, BadInterval { box_pos: usize }, BadTarget { box_pos: usize } }`, `engine::validate_system(boxes: &[BoxRule]) -> Result<(), SystemError>`, `engine::remap_positions(old_len: usize, origins: &[Option<usize>]) -> Vec<usize>` (index = old box position, value = new position; Plan 4 applies it with SQL `UPDATE`s)

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test engine`
Expected: compile error, `cannot find function validate_system` / `remap_positions`.

- [ ] **Step 3: Implement**

Add after the `Progress` struct:
```rust
#[derive(Debug, PartialEq, Eq)]
pub enum SystemError {
    NoBoxes,
    BadInterval { box_pos: usize },
    BadTarget { box_pos: usize },
}
```
Add after `schedule`:
```rust
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
```
`remap_positions` requires a non-empty `origins`; callers run `validate_system` first, which rejects empty systems.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test engine`
Expected: 14 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/engine.rs
git commit -m "feat: study system validation and box remapping"
```

---

### Task 4: Session order

**Files:**
- Modify: `src/engine.rs`

**Interfaces:**
- Produces: `engine::DueItem { card_id: i64, direction: u8, due: NaiveDate }`, `engine::order_session<R: rand::Rng>(items: Vec<DueItem>, rng: &mut R) -> Vec<DueItem>` (Plan 3 calls it with `rand::rng()`)

- [ ] **Step 1: Write the failing tests**

Add to the top of the `tests` module, after `use super::*;`:
```rust
    use rand::SeedableRng;
    use rand::rngs::StdRng;
```
Append to the `tests` module:
```rust
    fn item(card_id: i64, direction: u8, due: &str) -> DueItem {
        DueItem {
            card_id,
            direction,
            due: d(due),
        }
    }

    #[test]
    fn session_is_oldest_due_first() {
        let items = vec![
            item(1, 0, "2026-10-10"),
            item(2, 0, "2026-10-08"),
            item(3, 0, "2026-10-09"),
        ];
        let out = order_session(items, &mut StdRng::seed_from_u64(1));
        let ids: Vec<i64> = out.iter().map(|i| i.card_id).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn session_shuffles_ties() {
        let items: Vec<DueItem> = (1..=10).map(|c| item(c, 0, "2026-10-10")).collect();
        let orders: std::collections::HashSet<Vec<i64>> = (0..20)
            .map(|seed| {
                order_session(items.clone(), &mut StdRng::seed_from_u64(seed))
                    .iter()
                    .map(|i| i.card_id)
                    .collect()
            })
            .collect();
        assert!(orders.len() > 1);
    }

    #[test]
    fn session_never_puts_two_directions_of_a_card_back_to_back() {
        let items: Vec<DueItem> = (1..=3)
            .flat_map(|c| [item(c, 0, "2026-10-10"), item(c, 1, "2026-10-10")])
            .collect();
        for seed in 0..200 {
            let out = order_session(items.clone(), &mut StdRng::seed_from_u64(seed));
            assert_eq!(out.len(), 6);
            for pair in out.windows(2) {
                assert_ne!(pair[0].card_id, pair[1].card_id, "seed {seed}: {out:?}");
            }
        }
    }

    #[test]
    fn session_keeps_unavoidable_neighbours() {
        let items = vec![item(1, 0, "2026-10-10"), item(1, 1, "2026-10-10")];
        let out = order_session(items, &mut StdRng::seed_from_u64(0));
        assert_eq!(out.len(), 2);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test engine`
Expected: compile error, `cannot find type DueItem` / `cannot find function order_session`.

- [ ] **Step 3: Implement**

Add to the `use` lines at the top of `src/engine.rs`:
```rust
use rand::Rng;
use rand::seq::SliceRandom;
```
Add after the `Progress` struct:
```rust
/// A due card direction, as listed for a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DueItem {
    pub card_id: i64,
    pub direction: u8,
    pub due: NaiveDate,
}
```
Add after `remap_positions`:
```rust
/// Session order (§5.5): oldest due first, ties shuffled, and two directions of the
/// same card never back to back when the due-date order allows avoiding it.
// ponytail: O(n²) per due date, fine for hundreds of cards per session.
pub fn order_session<R: Rng>(mut items: Vec<DueItem>, rng: &mut R) -> Vec<DueItem> {
    items.shuffle(rng);
    items.sort_by_key(|i| i.due); // stable: shuffled order survives within a day
    let mut out: Vec<DueItem> = Vec::with_capacity(items.len());
    let mut start = 0;
    while start < items.len() {
        let day = items[start].due;
        let end = start + items[start..].iter().take_while(|i| i.due == day).count();
        let mut group = items[start..end].to_vec();
        while !group.is_empty() {
            let last = out.last().map(|i| i.card_id);
            let remaining = |card: i64| group.iter().filter(|i| i.card_id == card).count();
            // Prefer a different card than the last one, and among those the card with
            // most items left, so the final picks don't get stuck on one card.
            let pick = (0..group.len())
                .filter(|&k| Some(group[k].card_id) != last)
                .max_by_key(|&k| (remaining(group[k].card_id), std::cmp::Reverse(k)))
                .unwrap_or(0);
            out.push(group.remove(pick));
        }
        start = end;
    }
    out
}
```
Two directions of one card are only kept apart within the same due date. Moving one across dates would break "oldest due first", which the spec ranks higher.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test engine && cargo clippy --all-targets -- -D warnings`
Expected: 18 tests pass; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/engine.rs
git commit -m "feat: session ordering"
```

---
### Task 5: Schema, migrations and opening the database

**Files:**
- Create: `migrations/001_init.sql`, `src/db.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Produces: `db::open(path: &Path) -> Result<Connection, OpenError>`, `db::open_in_memory() -> Result<Connection, OpenError>`, `db::prepare(conn: Connection) -> Result<Connection, OpenError>` (Plan 5's restore runs `prepare` on the uploaded file), `db::OpenError { Sqlite(rusqlite::Error), NotWistjet, TooNew { found: usize, supported: usize } }`
- Schema facts later tasks rely on: study system `1` is the School 4-box preset; `deck.direction_mode` is one of `'front_to_back' | 'back_to_front' | 'both'`; box targets are stored as positions; a missing `progress` row means "first box, due today".

- [ ] **Step 1: Write the migration**

`migrations/001_init.sql` holds the whole MVP schema (later plans only add migrations for changes made after a release). The `CHECK` constraints are a last line of defence for the spec §8 limits; Plans 2–4 also validate at the API so users get friendly messages.
```sql
PRAGMA application_id = 1464423252; -- "WIST"

CREATE TABLE study_system (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 50),
    is_preset     INTEGER NOT NULL DEFAULT 0,
    repeat_missed INTEGER NOT NULL DEFAULT 1
);

-- Targets are box positions within the same system (0-based).
CREATE TABLE box (
    system_id        INTEGER NOT NULL REFERENCES study_system(id) ON DELETE CASCADE,
    position         INTEGER NOT NULL CHECK (position >= 0),
    name             TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 50),
    interval_days    INTEGER NOT NULL CHECK (interval_days BETWEEN 1 AND 365),
    success_position INTEGER NOT NULL CHECK (success_position >= 0),
    failure_position INTEGER NOT NULL CHECK (failure_position >= 0),
    PRIMARY KEY (system_id, position)
);

CREATE TABLE family (
    id                INTEGER PRIMARY KEY CHECK (id = 1),
    name              TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 50),
    timezone          TEXT NOT NULL,
    pin_hash          TEXT NOT NULL,
    default_system_id INTEGER NOT NULL REFERENCES study_system(id)
);

CREATE TABLE child (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 50),
    avatar   TEXT NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE subject (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 50),
    archived INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE deck (
    id             INTEGER PRIMARY KEY,
    subject_id     INTEGER NOT NULL REFERENCES subject(id),
    name           TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 50),
    system_id      INTEGER NOT NULL REFERENCES study_system(id),
    direction_mode TEXT NOT NULL CHECK (direction_mode IN ('front_to_back', 'back_to_front', 'both')),
    archived       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE deck_child (
    deck_id  INTEGER NOT NULL REFERENCES deck(id),
    child_id INTEGER NOT NULL REFERENCES child(id),
    PRIMARY KEY (deck_id, child_id)
);

CREATE TABLE image (
    id   INTEGER PRIMARY KEY,
    mime TEXT NOT NULL CHECK (mime IN ('image/jpeg', 'image/png', 'image/webp', 'image/gif')),
    data BLOB NOT NULL CHECK (length(data) <= 5242880)
);

CREATE TABLE card (
    id             INTEGER PRIMARY KEY,
    deck_id        INTEGER NOT NULL REFERENCES deck(id),
    front_text     TEXT NOT NULL DEFAULT '' CHECK (length(front_text) <= 500),
    front_image_id INTEGER REFERENCES image(id),
    back_text      TEXT NOT NULL DEFAULT '' CHECK (length(back_text) <= 500),
    back_image_id  INTEGER REFERENCES image(id),
    notes          TEXT NOT NULL DEFAULT '' CHECK (length(notes) <= 1000),
    active         INTEGER NOT NULL DEFAULT 1,
    CHECK (front_text <> '' OR front_image_id IS NOT NULL),
    CHECK (back_text <> '' OR back_image_id IS NOT NULL)
);
CREATE INDEX card_deck ON card(deck_id);

-- No row = never reviewed = first box, due today (spec §5.1).
-- direction: 0 = front → back, 1 = back → front.
CREATE TABLE progress (
    child_id         INTEGER NOT NULL REFERENCES child(id),
    card_id          INTEGER NOT NULL REFERENCES card(id),
    direction        INTEGER NOT NULL CHECK (direction IN (0, 1)),
    box_position     INTEGER NOT NULL,
    due_date         TEXT NOT NULL, -- YYYY-MM-DD in the family timezone
    last_reviewed_at TEXT,
    correct_count    INTEGER NOT NULL DEFAULT 0,
    incorrect_count  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (child_id, card_id, direction)
);

-- id is generated by the client per answer, so a retried submission is a no-op.
CREATE TABLE review_log (
    id         TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 64),
    child_id   INTEGER NOT NULL REFERENCES child(id),
    card_id    INTEGER NOT NULL REFERENCES card(id),
    direction  INTEGER NOT NULL,
    prev_box   INTEGER NOT NULL,
    new_box    INTEGER NOT NULL,
    known      INTEGER NOT NULL,
    local_date TEXT NOT NULL,
    at         TEXT NOT NULL
);
CREATE INDEX review_log_child_day ON review_log(child_id, local_date);

INSERT INTO study_system (id, name, is_preset, repeat_missed) VALUES (1, 'School 4-box', 1, 1);
INSERT INTO box (system_id, position, name, interval_days, success_position, failure_position) VALUES
    (1, 0, 'Box 1', 1, 1, 0),
    (1, 1, 'Box 2', 2, 2, 0),
    (1, 2, 'Box 3', 7, 3, 0),
    (1, 3, 'Box 4', 14, 3, 0);
```

- [ ] **Step 2: Write the failing tests**

Add to `src/lib.rs`:
```rust
pub mod db;
```
Create `src/db.rs` with the doc comment and the test module:
```rust
//! Opening the SQLite file and applying migrations (spec §9.1: upgrades migrate automatically).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_gets_schema_and_preset() {
        let conn = open_in_memory().unwrap();
        let boxes: i64 = conn
            .query_row("SELECT count(*) FROM box WHERE system_id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(boxes, 4);
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let conn = prepare(open_in_memory().unwrap()).unwrap();
        let systems: i64 = conn
            .query_row("SELECT count(*) FROM study_system", [], |r| r.get(0))
            .unwrap();
        assert_eq!(systems, 1);
    }

    #[test]
    fn rejects_database_from_newer_app() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA application_id = 1464423252; PRAGMA user_version = 99;")
            .unwrap();
        assert!(matches!(
            prepare(conn),
            Err(OpenError::TooNew { found: 99, .. })
        ));
    }

    #[test]
    fn rejects_foreign_sqlite_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 3;").unwrap();
        assert!(matches!(prepare(conn), Err(OpenError::NotWistjet)));
    }

    #[test]
    fn rejects_file_that_is_not_sqlite() {
        let path = std::env::temp_dir().join(format!("wistjet-garbage-{}.db", std::process::id()));
        std::fs::write(
            &path,
            b"this is not a database, just some text padding it out",
        )
        .unwrap();
        let result = open(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(result, Err(OpenError::Sqlite(_))));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test db::`
Expected: compile error, `cannot find function open_in_memory`.

- [ ] **Step 4: Implement**

Insert between the doc comment and `#[cfg(test)]` in `src/db.rs`:
```rust
use rusqlite::Connection;
use std::path::Path;

const MIGRATIONS: &[&str] = &[include_str!("../migrations/001_init.sql")];
const APPLICATION_ID: i64 = 1464423252; // "WIST", set by migration 001

#[derive(Debug)]
pub enum OpenError {
    Sqlite(rusqlite::Error),
    NotWistjet,
    TooNew { found: usize, supported: usize },
}

impl From<rusqlite::Error> for OpenError {
    fn from(e: rusqlite::Error) -> Self {
        OpenError::Sqlite(e)
    }
}

pub fn open(path: &Path) -> Result<Connection, OpenError> {
    prepare(Connection::open(path)?)
}

pub fn open_in_memory() -> Result<Connection, OpenError> {
    prepare(Connection::open_in_memory()?)
}

/// Checks the file is ours and not from a newer app, then migrates it to the latest schema.
pub fn prepare(mut conn: Connection) -> Result<Connection, OpenError> {
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let app_id: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    if version > 0 && app_id != APPLICATION_ID {
        return Err(OpenError::NotWistjet);
    }
    if version > MIGRATIONS.len() {
        return Err(OpenError::TooNew {
            found: version,
            supported: MIGRATIONS.len(),
        });
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(conn)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test db:: && cargo clippy --all-targets -- -D warnings`
Expected: 5 tests pass; clippy clean.

- [ ] **Step 6: Commit**

```bash
git add migrations src/db.rs src/lib.rs
git commit -m "feat: SQLite schema, preset and migrations"
```

---

### Task 6: Due cards query

**Files:**
- Create: `src/store.rs`, `tests/store.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `engine::DueItem` (Task 4), `db::open_in_memory` (Task 5)
- Produces: `store::due_items(conn: &Connection, child_id: i64, subject_id: Option<i64>, today: NaiveDate) -> rusqlite::Result<Vec<DueItem>>` (unordered; Plan 3 passes the result to `engine::order_session`), and the private `VISIBLE` SQL fragment reused by Task 7

- [ ] **Step 1: Write the failing tests**

Add to `src/lib.rs`:
```rust
pub mod store;
```
Create `tests/store.rs`:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test store`
Expected: compile error, `file not found for module store` (or `cannot find function due_items`).

- [ ] **Step 3: Implement**

Create `src/store.rs`:
```rust
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
```
`YYYY-MM-DD` strings sort the same as dates, so `<=` on text is a correct date comparison.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test store && cargo clippy --all-targets -- -D warnings`
Expected: 4 tests pass; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/store.rs src/lib.rs tests/store.rs
git commit -m "feat: due cards query"
```

---

### Task 7: Recording answers

**Files:**
- Modify: `src/store.rs`, `tests/store.rs`

**Interfaces:**
- Consumes: `engine::{schedule, new_progress, is_due, BoxRule, Progress}` (Task 1), `VISIBLE` (Task 6)
- Produces: `store::Answer { id: String, child_id: i64, card_id: i64, direction: u8, known: bool }`, `store::AnswerOutcome { Scheduled(Progress), Practice, AlreadyRecorded, NotFound }`, `store::record_answer(conn: &mut Connection, a: &Answer, now: DateTime<Utc>, today: NaiveDate) -> rusqlite::Result<AnswerOutcome>`, `store::load_boxes(conn: &Connection, system_id: i64) -> rusqlite::Result<Vec<BoxRule>>`. Plan 3's answer endpoint maps `NotFound` to HTTP 404 and every other outcome to 200.

- [ ] **Step 1: Write the failing tests**

In `tests/store.rs`, replace the `use` lines and constants with:
```rust
use chrono::{DateTime, Days, NaiveDate, Utc};
use rusqlite::Connection;
use wistjet::db;
use wistjet::engine::Progress;
use wistjet::store::{self, Answer, AnswerOutcome};

const LOTTE: i64 = 1;
const WOUT: i64 = 2;
const CHAT: i64 = 1;
const CHIEN: i64 = 2;
```
Add after `fn d`:
```rust
fn now() -> DateTime<Utc> {
    "2026-10-10T08:00:00Z".parse().unwrap()
}
```
Add after `fn fixture`:
```rust
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
```
Add after `fn due`:
```rust
fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}
```
Append at the end of the file:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test store`
Expected: compile error, `unresolved imports wistjet::store::Answer, wistjet::store::AnswerOutcome`.

- [ ] **Step 3: Implement**

In `src/store.rs`, replace the three `use` lines with:
```rust
use crate::engine::{self, BoxRule, DueItem, Progress};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, Result, TransactionBehavior, named_params, params};
```
Add after the `VISIBLE` constant:
```rust
pub struct Answer {
    /// Generated by the client once per answer; resubmitting it is a no-op.
    pub id: String,
    pub child_id: i64,
    pub card_id: i64,
    pub direction: u8,
    pub known: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AnswerOutcome {
    /// First answer while due: box and due date changed.
    Scheduled(Progress),
    /// Card direction was not due (already answered today): nothing changed.
    Practice,
    /// Same answer id seen before: nothing changed.
    AlreadyRecorded,
    /// Card direction is not visible to this child.
    NotFound,
}
```
Append at the end of the file:
```rust
pub fn load_boxes(conn: &Connection, system_id: i64) -> Result<Vec<BoxRule>> {
    let mut stmt = conn.prepare(
        "SELECT interval_days, success_position, failure_position
         FROM box WHERE system_id = ?1 ORDER BY position",
    )?;
    let rows = stmt.query_map([system_id], |r| {
        Ok(BoxRule {
            interval_days: r.get(0)?,
            success: r.get(1)?,
            failure: r.get(2)?,
        })
    })?;
    rows.collect()
}

/// Applies one answer (spec §5.3). Safe to call twice with the same `Answer::id`.
pub fn record_answer(
    conn: &mut Connection,
    a: &Answer,
    now: DateTime<Utc>,
    today: NaiveDate,
) -> Result<AnswerOutcome> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let seen: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM review_log WHERE id = ?1)",
        [&a.id],
        |r| r.get(0),
    )?;
    if seen {
        return Ok(AnswerOutcome::AlreadyRecorded);
    }
    let sql = format!(
        "SELECT d.system_id, p.box_position, p.due_date {VISIBLE} AND c.id = :card AND x.dir = :dir"
    );
    let found = tx
        .query_row(
            &sql,
            named_params! { ":child": a.child_id, ":card": a.card_id, ":dir": a.direction },
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<usize>>(1)?,
                    r.get::<_, Option<NaiveDate>>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((system_id, box_pos, due)) = found else {
        return Ok(AnswerOutcome::NotFound);
    };
    let current = match (box_pos, due) {
        (Some(box_pos), Some(due)) => Progress { box_pos, due },
        _ => engine::new_progress(today),
    };
    if !engine::is_due(current, today) {
        return Ok(AnswerOutcome::Practice);
    }
    let next = engine::schedule(
        &load_boxes(&tx, system_id)?,
        current.box_pos,
        a.known,
        today,
    );
    tx.execute(
        "INSERT INTO progress (child_id, card_id, direction, box_position, due_date,
                               last_reviewed_at, correct_count, incorrect_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (child_id, card_id, direction) DO UPDATE SET
           box_position = excluded.box_position,
           due_date = excluded.due_date,
           last_reviewed_at = excluded.last_reviewed_at,
           correct_count = correct_count + excluded.correct_count,
           incorrect_count = incorrect_count + excluded.incorrect_count",
        params![
            a.child_id,
            a.card_id,
            a.direction,
            next.box_pos,
            next.due,
            now,
            i64::from(a.known),
            i64::from(!a.known)
        ],
    )?;
    tx.execute(
        "INSERT INTO review_log (id, child_id, card_id, direction, prev_box, new_box, known, local_date, at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            a.id,
            a.child_id,
            a.card_id,
            a.direction,
            current.box_pos,
            next.box_pos,
            a.known,
            today,
            now
        ],
    )?;
    tx.commit()?;
    Ok(AnswerOutcome::Scheduled(next))
}
```
Why there is no "first answer of the day" table: every interval is at least 1 day, so a scheduled card is never due again the same day, and `is_due` alone separates scheduling answers from practice. The `Immediate` transaction makes two concurrent answers for one card run one after the other. Returning early drops `tx`, which rolls it back.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test store && cargo clippy --all-targets -- -D warnings`
Expected: 13 tests pass; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/store.rs tests/store.rs
git commit -m "feat: record answers idempotently"
```

---

### Task 8: Reset progress

**Files:**
- Modify: `src/store.rs`, `tests/store.rs`

**Interfaces:**
- Consumes: `record_answer` (Task 7)
- Produces: `store::reset_progress(conn: &Connection, deck_id: i64, child_id: Option<i64>, today: NaiveDate) -> rusqlite::Result<usize>` (`None` = all children; Plan 4 wires it to the reset buttons after a confirmation dialog)

- [ ] **Step 1: Write the failing tests**

In `tests/store.rs`, add after `const WOUT`:
```rust
const DECK: i64 = 1;
```
Append at the end of the file:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test store`
Expected: compile error, `cannot find function reset_progress`.

- [ ] **Step 3: Implement**

Append to `src/store.rs`:
```rust
/// Resets a deck to the first box, due today (spec §5.7): one child, or all (`None`).
/// Directions without a progress row are already in that state. The review log is kept.
pub fn reset_progress(
    conn: &Connection,
    deck_id: i64,
    child_id: Option<i64>,
    today: NaiveDate,
) -> Result<usize> {
    conn.execute(
        "UPDATE progress SET box_position = 0, due_date = ?3
         WHERE card_id IN (SELECT id FROM card WHERE deck_id = ?1)
           AND (?2 IS NULL OR child_id = ?2)",
        params![deck_id, child_id, today],
    )
}
```
Correct/incorrect counts are kept on purpose; the spec only asks for box and due date to reset.

- [ ] **Step 4: Run the full suite**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: 23 unit tests + 16 store tests pass; fmt and clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/store.rs tests/store.rs
git commit -m "feat: reset progress per child or per deck"
```

---

## Done when

- `cargo test` passes all 39 tests, and CI is green on the pushed branch.
- Every rule in spec §5 has a test: transitions (Task 1), timezone/DST (Task 2), validation and box remapping (Task 3), session order (Task 4), due/hidden/directions (Tasks 6–7), answering, duplicates and practice (Task 7), reset (Task 8).
- Spec §11 items left for later plans: repeat-missed-cards on/off is client queue behaviour (Plan 3); applying the remap to stored progress (Plan 4); end-to-end, backup round trip and PIN enforcement tests (Plans 2 and 5).
