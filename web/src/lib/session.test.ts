import assert from 'node:assert/strict'
import { test } from 'node:test'
import { answer, newId, skip, start, type StudyCard } from './session.ts'

function card(card_id: number, repeat_missed = true): StudyCard {
  const side = { text: `kaart ${card_id}`, image: null }
  return { card_id, direction: 0, subject: 'Frans', deck: 'H1', question: side, answer: side, notes: '', repeat_missed }
}

let n = 0
const ids = () => `id${++n}`

test('first showings carry an answer id', () => {
  const q = start([card(1), card(2)], ids)
  assert.equal(q.turns.length, 2)
  assert.ok(q.turns.every((t) => t.answerId !== null))
  assert.notEqual(q.turns[0].answerId, q.turns[1].answerId)
})

test('known cards leave the queue and are counted', () => {
  let q = start([card(1), card(2)], ids)
  q = answer(q, true)
  q = answer(q, true)
  assert.equal(q.index, 2)
  assert.equal(q.turns.length, 2)
  assert.deepEqual(q.summary, { known: 2, notKnown: 0, repeats: 0 })
})

test('missed cards return at the end until known once', () => {
  let q = start([card(1), card(2)], ids)
  q = answer(q, false) // card 1 missed
  assert.equal(q.turns.length, 3)
  assert.equal(q.turns[2].card.card_id, 1)
  assert.equal(q.turns[2].answerId, null) // practice: not sent to the server
  q = answer(q, true) // card 2
  q = answer(q, false) // card 1 repeat, missed again
  assert.equal(q.turns.length, 4)
  q = answer(q, true) // card 1 repeat, known
  assert.equal(q.index, q.turns.length)
  assert.deepEqual(q.summary, { known: 1, notKnown: 1, repeats: 2 })
})

test('missed cards leave when the study system does not repeat them', () => {
  let q = start([card(1, false)], ids)
  q = answer(q, false)
  assert.equal(q.turns.length, 1)
  assert.deepEqual(q.summary, { known: 0, notKnown: 1, repeats: 0 })
})

test('answering past the end changes nothing', () => {
  const q = start([], ids)
  assert.equal(answer(q, true), q)
})

test('new ids are 32 hex characters and differ', () => {
  const a = newId()
  assert.match(a, /^[0-9a-f]{32}$/)
  assert.notEqual(a, newId())
})

test('a card the server no longer has is skipped without counting', () => {
  let q = start([card(1), card(2)], ids)
  q = skip(q)
  assert.equal(q.index, 1)
  assert.equal(q.turns[q.index].card.card_id, 2)
  assert.deepEqual(q.summary, { known: 0, notKnown: 0, repeats: 0 })
  q = skip(skip(q))
  assert.equal(q.index, 2)
})
