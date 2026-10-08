# Family Flashcards — MVP Design

Date: 2026-10-08
Status: Draft for review

## 1. Purpose

An open-source, self-hosted flashcard app for children who already use the
physical 4-box rehearsal system at school. It reproduces that workflow in the
browser so a child only has to answer one question: *what do I need to
practise today?*

Parents manage content and settings. Children select their profile, flip
cards, and say whether they knew the answer.

Product principle: stay simpler than general-purpose flashcard tools. Simple,
beautiful, practical.

This spec describes behaviour only. Technology choices are made during
implementation planning.

## 2. Context and constraints

- One installation serves one family on a trusted home network.
- Other families must be able to self-host it with minimal effort: no external
  accounts or cloud services required.
- Primary devices: laptop and tablet browsers. Must remain usable on a phone
  browser. No native apps.
- Interface language: Dutch only for the MVP. No translation framework.
- No user accounts. Parent settings are protected by a PIN.

## 3. Users

**Parent** — sets up the family, children, subjects, decks, cards, and study
systems; views progress; manages backups.

**Child** — selects their profile, studies due cards, sees a short summary.
Never sees boxes, intervals, or dates.

## 4. Concepts

### Family
Exactly one per installation. Fields: name, timezone, parent PIN (stored
hashed), default study system for new decks.

### Child
Display name, avatar or colour, active/archived. No other personal data.
Archived children are hidden from the profile screen; their progress is kept.

### Subject
Name, active/archived. Groups decks (e.g. Frans, Wiskunde, WO).

### Deck
Name, subject, study system, direction mode, assigned children,
active/archived.

Direction mode is one of:
- front → back
- back → front
- both (each direction has independent progress)

### Card
Belongs to one deck. Fields: front text, optional front image, back text,
optional back image, optional notes, active/inactive.

A card side must have text or an image (or both).

### Study system
An ordered list of one or more boxes plus one setting:
**repeat missed cards in this session** (default: on).

Each box has: name, position, interval in days (integer ≥ 1), success target
box, failure target box. Targets must be boxes within the same system.

### Built-in preset: School 4-box

| Box | Interval | Known → | Not known → |
|-----|----------|---------|-------------|
| 1   | 1 day    | Box 2   | Box 1       |
| 2   | 2 days   | Box 3   | Box 1       |
| 3   | 7 days   | Box 4   | Box 1       |
| 4   | 14 days  | Box 4   | Box 1       |

Presets are read-only. A parent can copy a preset into a custom system and
edit the copy. The preset is the initial family default.

### Progress
One record per (child, card, direction). Fields: current box, due date
(calendar day), last reviewed at, correct count, incorrect count.

### Review log
One entry per scheduling answer: child, card, direction, previous box, new
box, result (known / not known), timestamp. Practice repeats (see 5.4) are
not logged; they are counted only in the session summary.

## 5. Scheduling rules

All "day" calculations use calendar days in the family's timezone.

### 5.1 New card directions
A card direction a child has never reviewed starts in the system's first box
and is due today. Adding a card to an assigned deck, assigning a deck to a
child, or switching a deck to "both" therefore makes the new directions due
immediately.

### 5.2 Due
A card direction is due for a child when:
- the child is active and assigned to the deck,
- the deck, its subject, and the card are active,
- the direction is enabled by the deck's direction mode,
- its due date is today or earlier.

Overdue cards stay due until answered. Nothing expires; the schedule does not
depend on the child opening the app every day.

### 5.3 Answering
The first answer for a card direction on a given day is a **scheduling
answer**:
1. Known → move to the current box's success target. Not known → move to its
   failure target.
2. New due date = today + destination box interval.
3. Update counts and last-reviewed time; write a review log entry.

Because every interval is at least one day, a card direction is not due again
on the same day after a scheduling answer. Any further answer to it that day
is a practice answer and does not change progress.

Submitting the same answer twice (double tap, network retry) must have the
effect of a single answer.

### 5.4 Repeat missed cards
When the study system's "repeat missed cards" setting is on, a card answered
"not known" is put back at the end of the current session queue. It keeps
returning until the child answers "known" once. These repeats are practice
answers: they never change box or due date.

When the setting is off, a missed card leaves the session after its
scheduling answer.

### 5.5 Session order
A session is built when the child presses Start, from all due cards (or one
subject's due cards if the child chose a subject). Order: oldest due date
first; ties shuffled. Two directions of the same card should not appear back
to back when avoidable. Missed-card repeats go to the end of the queue.

A child may stop at any time. Answers already given are saved; unanswered
cards remain due.

### 5.6 Changing a study system in use
- Changing intervals or targets affects future answers only. Existing due
  dates are kept.
- A system is saved as a whole. Saving is rejected if any target points to a
  box not in the system, or if the system would have no boxes.
- When a box is removed, progress in that box moves to the box now at the
  same position; if no box exists at that position, to the last box. Due
  dates are kept.
- When a deck switches to another study system, each progress record moves
  to the box at the same position in the new system, or the last box if that
  position does not exist. Due dates are kept.
- A study system used by any deck, and the family default system, cannot be
  deleted.

### 5.7 Reset progress
The parent can reset:
- one child on one deck, or
- a whole deck for all assigned children.

Reset puts every affected card direction in the first box, due today. The
review log is kept. Reset requires confirmation.

### 5.8 Hiding and restoring
Making a card inactive, archiving a deck or subject, archiving a child,
unassigning a child from a deck, or narrowing a deck from "both" to one
direction hides the affected progress but never deletes it. Restoring
resumes from the stored progress.

## 6. Child experience

Default screen when the app opens. No PIN.

### 6.1 Profile select ("Wie gaat er oefenen?")
Large tiles per active child: name, avatar/colour, badge with cards due
today. A small lock icon opens the parent area.

### 6.2 Child home
- Greeting with the child's name.
- Total cards due today.
- Due count per subject; tapping a subject studies only that subject.
- Progress bar for today (answered vs. due at start of day).
- "Start met oefenen" button.
- When nothing is due: a cheerful "Klaar voor vandaag" message and when the
  next cards come up.

### 6.3 Study
Before reveal: subject, deck name, front (text and/or image), position
("12 van 23"), "Toon antwoord".

After reveal: front and back, notes if present, buttons "Wist ik niet" and
"Wist ik".

Keyboard: space reveals; ← = not known; → = known. Touch-friendly buttons on
tablets. No boxes, intervals, or dates shown.

If saving an answer fails, show a gentle retry message and keep the card on
screen. Never drop an answer silently.

### 6.4 Summary
Cards known, cards not known, number of practice repeats. "Klaar" returns to
profile select.

## 7. Parent area

Entered via PIN. Locks after 10 minutes of inactivity or via an explicit
"Vergrendel" button.

### 7.1 Overview
Per child: due today, reviewed today, known/not known today, number of active
card directions per box.

### 7.2 Children
Create, rename, set avatar/colour, archive, restore.

### 7.3 Subjects
Create, rename, archive, restore.

### 7.4 Decks
- Create/edit: name, subject, study system, direction mode, assigned
  children.
- Archive/restore.
- Deck page:
  - card list with add, edit, inactivate/reactivate, image upload per side;
  - **Paste a list** (see 7.5);
  - box distribution per assigned child, and due count;
  - reset progress (5.7).

### 7.5 Paste a list
The parent pastes text, one card per line. Each line is split on the first
tab; if there is no tab, on the first ` - `; if neither, on the first ` = `.
Left part = front, right part = back, both trimmed.

A preview shows the cards to be created and flags:
- lines that cannot be split (excluded),
- empty front or back (excluded),
- fronts that already exist in the deck (included, marked as possible
  duplicate; the parent can untick them).

Nothing is saved until the parent confirms. Images are added per card
afterwards.

### 7.6 Study systems
- List presets (read-only, copyable) and custom systems.
- Edit custom system: add, remove, reorder boxes; edit name and interval;
  choose success and failure targets per box; toggle "repeat missed cards".
- When a box is removed in the editor, targets that pointed to it are reset
  to defaults (success → next box or itself if last; failure → first box) and
  shown to the parent before saving.
- Set the family default system.

### 7.7 Settings
Family name, timezone, change PIN (requires current PIN), backup and
restore (section 9).

### 7.8 First run
When no family exists:
1. Family name, timezone (pre-filled from the browser), choose PIN.
2. Add first child.
3. Offer a small sample French–Dutch deck (assigned to that child) or start
   empty.

## 8. Security

Scope: trusted home network, no accounts. The README must state the app is
not meant to be exposed to the internet.

- The server enforces the PIN for every parent action. Without a parent
  session, a client can only: list children, read a child's due cards and
  summary data, and submit answers.
- PIN stored hashed. After 5 wrong attempts, PIN entry is blocked for
  1 minute.
- Parent sessions expire after 10 minutes of inactivity.
- All input validated: text lengths (front/back ≤ 500 characters, notes ≤
  1000, names ≤ 50), images limited to common web formats and ≤ 5 MB, box
  targets within the same system, intervals integer 1–365.
- Only a display name and avatar/colour are stored per child.

## 9. Data safety and self-hosting

### 9.1 Install and upgrade
- One command or one short config file to run it, both on a single machine
  and on a home cluster. Documented in the README.
- All data, including images, persists across restarts and upgrades in one
  configurable location.
- Upgrades apply any data changes automatically and keep existing data.

### 9.2 Backup and restore
- **Download backup** (Settings): one file containing all content, images,
  progress, review log, and settings.
- **Restore**: upload a backup file; after confirmation it replaces all
  current data. A backup from an older app version must restore into a newer
  one.
- **Automatic backup**: daily, written to a configurable folder (e.g. a
  mounted NAS share), keeping the last 14 copies by default (configurable).

## 10. Error handling

- Failed answer save: retry message, card stays; answer is never lost
  silently.
- Duplicate answer submissions count once.
- Blocked destructive actions explain why (e.g. "Dit studiesysteem wordt
  gebruikt door 3 decks").
- Restore of an invalid or unreadable file fails with a clear message and
  leaves current data untouched.

## 11. Testing

Automated tests for every scheduling rule in section 5, including:
- box transitions and due dates for known / not known;
- timezone handling, including daylight-saving transitions and answers near
  midnight;
- overdue accumulation over skipped days;
- first-answer-of-day vs. practice answers;
- repeat-missed-cards on and off;
- box removal and system switching (position mapping, last-box fallback);
- reset scopes;
- independent progress per child and per direction;
- duplicate answer submission counted once.

Plus:
- one end-to-end test: first run → create deck via paste → study a session
  → summary;
- backup → restore round trip yields identical data;
- PIN enforcement: parent actions rejected without a parent session.

## 12. Out of scope for MVP

User accounts, multiple families, typed-answer checking, multiple choice,
audio, spelling mode, OCR / PDF / AI card creation, file import/export,
notifications and reminders, streaks and gamification, translations beyond
Dutch, child PINs, native apps, public deck sharing, advanced analytics,
alternative spaced-repetition algorithms.

## 13. Acceptance criteria

Parent
- [ ] First run sets family name, timezone, PIN, first child, optional sample deck.
- [ ] Parent area requires the PIN, enforced by the server, and auto-locks.
- [ ] Parent can create, rename, archive children and subjects.
- [ ] Parent can create decks, choose subject, study system, direction mode, assigned children.
- [ ] Parent can add, edit, inactivate cards, with optional image per side.
- [ ] Parent can paste a list, preview it, and create cards.
- [ ] Parent can copy the preset and create/edit custom study systems (boxes, intervals, targets, repeat setting).
- [ ] Parent can reset progress per child per deck and per deck.
- [ ] Parent sees per-child overview and per-deck box distribution.
- [ ] Parent can download a backup and restore it.

Child
- [ ] Child selects their profile without a PIN.
- [ ] Child sees total due and due per subject.
- [ ] Child can study all due cards or one subject.
- [ ] Child reveals the answer and marks it known / not known (mouse, touch, keyboard).
- [ ] Next card appears automatically; missed cards repeat when enabled.
- [ ] Summary appears at the end; stopping early keeps answers.

Learning engine
- [ ] All rules in section 5 hold and are covered by automated tests.

Self-hosting
- [ ] Runs from one command/config on a single machine and on a home cluster.
- [ ] Data persists across restarts and upgrades.
- [ ] Daily automatic backup to a configurable folder with retention.
- [ ] README documents install, upgrade, backup/restore, and the no-internet-exposure warning.
