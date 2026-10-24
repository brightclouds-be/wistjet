# Wistjet — Multiplication and division tables ("tafels")

Date: 2026-10-24. Extends the main spec
(`2026-10-08-family-flashcards-design.md`); section numbers below refer to it.

## 1. Intent

Kids in Flanders have to "automatiseren" their multiplication and division
tables: the answer must come instantly, not be worked out. Some schools give
an app for this (our son uses CalcuPal: a sum appears, he types the answer as
fast as he can, the time is measured). Many schools only practise on paper,
so those kids have nothing at home.

Wistjet should be the one place for practising at home: flashcards and
tables together, in the same boxes, on the same child home screen.

A sum is **automated** when the child answers it right *and* fast enough.
Only then does it move up a box. A sum the child gets right but still has to
think about stays in the early boxes and comes back daily, so practice
concentrates on exactly the sums that are not automatic yet.

## 2. Goals

- A parent sets up table practice in a few clicks, without typing sums.
- Multiplication and division tables 1 to 10.
- Every sum is typed and timed; known = right and within the time limit.
- Time limit per child, 3 seconds by default, changeable by a parent.
- Sums live in the boxes like any card: same scheduling, same due logic,
  same box bar on the child home and summary.

## 3. Non-goals

- A separate timed-drill mode outside the boxes (score per minute, races).
- Parent statistics per sum ("6 × 7 still slow"). The box bar is enough for
  now.
- Other arithmetic (sums above the tables, addition, subtraction).
- Tables beyond 10, or 0 × n.

## 4. Anti-goals

- Stress: no countdown or ticking timer visible while the child thinks.
- A slow child stuck in an endless repeat loop within one session.
- Parents having to type 200 cards.

## 5. Concepts

### Deck: answer mode
A deck gets one new setting, **answer mode**:
- **Zelf beoordelen** (default, as today): reveal, then "Wist ik niet" /
  "Wist ik", with the random typed check of §6.3.
- **Typen op tijd**: every showing asks the child to type the answer; known
  = typed answer right and given within the child's time limit.

A "Typen op tijd" deck always studies front → back (direction mode is fixed
and not shown). Its cards are ordinary cards: the parent can still add,
edit, paste or inactivate them by hand.

### Child: time limit
Each child gets a **time limit** for "Typen op tijd" decks: default 3
seconds, settable by a parent from 1 to 30 seconds in half-second steps.

## 6. Parent: adding tables

On the page of a "Typen op tijd" deck, next to "Paste a list", a **Tafels
toevoegen** panel:
- tick tables 1 to 10 (any combination);
- tick "×" (maaltafels), ":" (deeltafels), or both;
- preview the sums to be created, then confirm.

The table of n gives:
- multiplication: `1 × n`, `2 × n`, … `10 × n`, back = the product;
- division: `n : n`, `2n : n`, … `10n : n`, back = the quotient.

Written with the "×" and ":" signs as on Flemish school sheets, with spaces
around the sign. Sums whose front already exists in the deck are skipped
(and shown as such in the preview), so ticking table 4 twice, or tables that
overlap, never creates duplicates.

A natural setup is one subject "Wiskunde" with a deck "Tafels" in answer
mode "Typen op tijd", assigned to the child. The parent can make several
decks (e.g. "Maaltafels" and "Deeltafels") if they want them separate.

## 7. Child: practising a sum

In a session, cards from "Typen op tijd" decks are mixed with other due cards
as usual (§5.5). For such a card:

- Shown: subject, deck, position, the sum ("4 × 6"), an answer field with
  the focus in it, and "Controleer". Tablets show the number keyboard.
- The time runs from the moment the sum is shown until the child submits
  (Enter or "Controleer"). It is not shown while the child thinks.
- After submitting, the answer side is shown with one of:
  - right and within the limit: "✓ Goed!" (green) — known;
  - right but too slow: "Goed, maar nog wat sneller! (4,2 s)" (orange) —
    not known;
  - wrong: "Het is 24" (red) — not known;
  and a single "Verder" button. The child cannot override the result.
- No arrow or space shortcuts; Enter submits, then Enter on "Verder"
  continues.

Matching: the typed answer is compared as for the typed check (§6.3: case,
spacing and punctuation don't count), so "24", " 24 " are right.

### Repeats in the same session
When the study system repeats missed cards (§5.4), a wrong or slow sum comes
back at the end of the session. The repeat is practice, as today. A repeat
ends the loop as soon as the answer is **right**, however long it took; the
time is still shown ("Goed! (5,1 s)"). Speed only counts for the scheduling
answer, so a slow child is never stuck in the loop (anti-goal), while the
sum still returns tomorrow.

## 8. What changes elsewhere

- **Overview warning (§7.1)**: answers from "Typen op tijd" decks are left
  out of the "clicking through" count; answering them fast is the point.
- **Typed check (§6.3)**: does not apply to "Typen op tijd" decks; every
  showing is already typed.
- **Box bar, summary, "⬆ N kaartjes een bakje verder"**: unchanged; sums are
  cards.
- **Sample deck / first run**: unchanged.

## 9. Left to the builder

- Where the time is measured. The child's device may measure it; the server
  records known / not known as today. No answer times need to be stored.
- Exact layout and wording of the "Tafels toevoegen" panel and of the time
  limit field on the child edit form, within the existing look.
- Whether changing an existing deck's answer mode is allowed (it may be; its
  progress stays as it is).

## 10. Acceptance criteria

- A parent creates a "Typen op tijd" deck, adds tables 3 and 4 for × and :,
  and gets 40 cards; adding table 4 again adds none.
- The child is shown "4 × 6", types 24 within the limit: "✓ Goed!", the card
  moves up a box.
- Right after the limit: orange message with the time, card stays in or goes
  back to the first box, and returns at the end of the session.
- On that repeat, a right answer ends the loop regardless of time.
- Wrong answer: "Het is 24", not known.
- Changing the child's time limit to 5 s makes a 4-second right answer known.
- Fast answers on a "Typen op tijd" deck never trigger the overview warning.
- A "Zelf beoordelen" deck behaves exactly as before.
