# Wistjet — Technical Plan & Roadmap

**Spec:** `docs/superpowers/specs/2026-10-08-family-flashcards-design.md`

This document fixes the technology choices and splits the MVP into five
implementation plans. Each plan ends in working, tested software. Only Plan 1
is written out in full; each following plan is written when the previous one
has landed, so it can build on the real interfaces instead of guessed ones.

## Stack

| Concern | Choice | Reason |
|---|---|---|
| Server | Rust (edition 2024), `axum` | One small static binary per platform; no runtime for families to install. |
| UI | Svelte 5 + Vite + TypeScript, built as a static SPA | Interactive study and editor screens with little code. Node is needed at build time only. |
| Embedding | `rust-embed` | The built SPA lives inside the binary: one file to download. |
| Database | SQLite via `rusqlite` (`bundled`) | No database server. One file holds everything, images included (BLOBs). |
| Migrations | Numbered `migrations/NNN.sql`, applied on start, tracked in `PRAGMA user_version` | Automatic upgrades (§9.1); old backups upgrade on restore (§9.2). |
| Dates | `chrono` + `chrono-tz`; due dates stored as `YYYY-MM-DD` in the family timezone | Calendar-day arithmetic can't be broken by daylight-saving changes. |
| PIN | `argon2` hash; in-memory parent sessions + attempt counter | One process, so no shared session store. A restart locks the parent area. |
| Tests | `cargo test` (engine + store); Playwright for the single end-to-end test | |
| Releases | `cargo-dist` via GitHub Actions: Windows/macOS/Linux binaries + installers; Docker image | Non-technical families: download and double-click. NAS / home-lab: container. |

## Architecture rules

- **Scheduling rules are pure functions** in `src/engine.rs`: no database, no
  clock. Callers pass `today`. All of spec §11's rule tests live there or in
  the store tests.
- **One connection behind a `Mutex`.** One family generates very little load. Every write
  is a short transaction.
- **The session queue lives in the browser.** The server returns ordered due
  cards; the client handles missed-card repeats (§5.4), the position counter
  and the summary. The server only stores scheduling answers.
- **Idempotent answers.** The client generates an id per answer;
  `review_log.id` is the primary key, so retries are no-ops (§5.3, §10).
- **Backup = the SQLite file.** Download is `VACUUM INTO` a temp file.
  Restore checks `application_id`, migrates the uploaded copy, then atomically
  swaps it in. Auto-backup is an in-process daily timer with retention.
- **Data location:** `WISTJET_DATA_DIR`, defaulting to the OS per-user data
  folder (`directories` crate). Also `WISTJET_BACKUP_DIR` and `WISTJET_PORT`
  (default 8080).
- **Single instance only.** The k8s example uses `replicas: 1` and
  `strategy: Recreate` with a PVC; the README says why.

## Plans

1. **Learning engine & storage** — crate setup, CI, the pure scheduling engine,
   schema + migrations, due/answer/reset queries. Covers every §5 rule and its
   §11 tests. → `2026-10-11-plan-1-learning-engine.md`
2. **App shell, first run & parent auth** — `axum` server, embedded Svelte SPA,
   launcher (data dir, opens the browser, shows the LAN URL + QR code for
   tablets, tray icon, no console window on Windows), first-run wizard
   (§7.8, including the sample French–Dutch deck), PIN hashing, lockout
   after 5 attempts, 10-minute parent sessions, server-side guard on every
   parent route (§8).
3. **Child experience** — profile select with due badges, child home with
   per-subject counts and today's progress bar, study screen (reveal,
   keyboard ← / → / space, touch buttons), repeat-missed queue, gentle retry on
   a failed save, summary (§6).
4. **Parent area** — overview, children, subjects, decks, cards with images,
   paste-a-list with preview (§7.5), study-system editor with box remapping
   (`engine::remap_positions` applied in one transaction), deck system switch,
   reset progress, settings, change PIN (§7).
5. **Data safety & distribution** — backup download/restore, daily automatic
   backup with retention (§9.2), Docker image, `compose.yaml`, k8s example,
   `cargo-dist` releases, README (install, upgrade, backup, no-internet
   warning), the Playwright end-to-end test (§11).

## Open decisions (not blocking Plan 1)

- **License** — the spec says open source but doesn't name one (MIT,
  Apache-2.0 or GPL-3.0). Needed before the first public release.
- **Code signing** for macOS/Windows installers — unsigned for the MVP; the
  README shows how to get past the "unknown developer" warning.
