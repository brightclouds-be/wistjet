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
}

/** One showing of a card. First showings carry the id the server records the answer
 *  under; repeats of missed cards are practice (spec §5.4) and are never sent. */
export type Turn = { card: StudyCard; answerId: string | null }

export type Summary = { known: number; notKnown: number; repeats: number }

export type Queue = { turns: Turn[]; index: number; summary: Summary }

/** Random hex id. `crypto.randomUUID` needs a secure context, which plain http on the
 *  home network is not; `getRandomValues` works everywhere. */
export function newId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

export function start(cards: StudyCard[], id: () => string = newId): Queue {
  return {
    turns: cards.map((card) => ({ card, answerId: id() })),
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
    turns: repeat ? [...q.turns, { card: turn.card, answerId: null }] : q.turns,
    index: q.index + 1,
    summary: s,
  }
}

/** The queue without the current turn, uncounted: the server no longer has the card for
 *  this child (archived or unassigned meanwhile), so its answer can never be saved. */
export function skip(q: Queue): Queue {
  return q.index < q.turns.length ? { ...q, index: q.index + 1 } : q
}
