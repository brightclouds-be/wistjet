/** A card side as the server sends it. */
export type Side = { text: string; image: number | null }

/** One due card direction, turned so `question` is shown first. */
export type StudyCard = {
  card_id: number
  direction: number
  subject: string
  deck: string
  question: Side
  answer: Side
  notes: string
  repeat_missed: boolean
  /** Set on "Typen op tijd" decks: every showing is typed and timed (tables spec §7). */
  time_limit_ms: number | null
}

/** One showing of a card. First showings carry the id the server records the answer
 *  under; repeats of missed cards are practice (spec §5.4) and are never sent. `check`
 *  turns ask the child to type the answer instead of judging it themselves (spec §6.3). */
export type Turn = { card: StudyCard; answerId: string | null; check: boolean }

export type Summary = { known: number; notKnown: number; repeats: number }

export type Queue = { turns: Turn[]; index: number; summary: Summary }

/** Random hex id. `crypto.randomUUID` needs a secure context, which plain http on the
 *  home network is not; `getRandomValues` works everywhere. */
export function newId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

/** Answers short enough to type; longer ones are never checked. */
const TYPEABLE = 40

export function start(cards: StudyCard[], id: () => string = newId, roll = Math.random): Queue {
  return {
    turns: cards.map((card) => {
      const text = card.answer.text.trim()
      const check = roll() < 0.2 && text !== '' && text.length <= TYPEABLE && card.time_limit_ms === null
      return { card, answerId: id(), check }
    }),
    index: 0,
    summary: { known: 0, notKnown: 0, repeats: 0 },
  }
}

/** The queue after answering the current turn. A missed card goes to the end of the
 *  queue when its study system repeats missed cards, until it is known once. */
export function answer(q: Queue, known: boolean): Queue {
  const turn = q.turns[q.index]
  if (!turn) return q
  const s = { ...q.summary }
  if (turn.answerId === null) s.repeats += 1
  else if (known) s.known += 1
  else s.notKnown += 1
  const repeat = !known && turn.card.repeat_missed
  return {
    turns: repeat ? [...q.turns, { card: turn.card, answerId: null, check: false }] : q.turns,
    index: q.index + 1,
    summary: s,
  }
}

/** The queue without the current turn, uncounted: the server no longer has the card for
 *  this child (archived or unassigned meanwhile), so its answer can never be saved. */
export function skip(q: Queue): Queue {
  return q.index < q.turns.length ? { ...q, index: q.index + 1 } : q
}

const plain = (s: string) =>
  s
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim()

/** Whether a typed answer matches the card's answer, or one of the answers it lists
 *  ("de hond, het hondje"). Case, accents and punctuation don't count. */
export function typedRight(typed: string, answer: string): boolean {
  const t = plain(typed)
  return t !== '' && [answer, ...answer.split(/[,;/]/)].some((a) => plain(a) === t)
}

const seconds = (ms: number) => `${(ms / 1000).toFixed(1).replace('.', ',')} s`

/** The result of a timed answer (tables spec §7). Speed only counts for the scheduling
 *  answer: a practice repeat that is right ends the repeat loop however long it took. */
export function timedVerdict(
  typed: string,
  answer: string,
  ms: number,
  limitMs: number,
  practice: boolean,
): { known: boolean; tone: 'yes' | 'late' | 'no'; message: string } {
  if (!typedRight(typed, answer)) return { known: false, tone: 'no', message: `Het is ${answer}` }
  if (practice) return { known: true, tone: 'yes', message: `Goed! (${seconds(ms)})` }
  if (ms > limitMs) return { known: false, tone: 'late', message: `Goed, maar nog wat sneller! (${seconds(ms)})` }
  return { known: true, tone: 'yes', message: '✓ Goed!' }
}
