import assert from 'node:assert/strict'
import { test } from 'node:test'
import { answer, newId, numeric, skip, start, timedVerdict, typedRight, type StudyCard } from './session.ts'

function card(card_id: number, repeat_missed = true): StudyCard {
  const side = { text: `kaart ${card_id}`, image: null }
  return { card_id, direction: 0, subject: 'Frans', deck: 'H1', question: side, answer: side, notes: '', repeat_missed, time_limit_ms: null }
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

test('about one in five first showings asks to type the answer', () => {
  const rolls = [0.1, 0.5, 0.19, 0.2]
  const q = start([card(1), card(2), card(3), card(4)], ids, () => rolls.shift()!)
  assert.deepEqual(
    q.turns.map((t) => t.check),
    [true, false, true, false],
  )
})

test('only short text answers are typed', () => {
  const long = { ...card(1), answer: { text: 'een heel lang antwoord dat niemand zo kan typen', image: null } }
  const picture = { ...card(2), answer: { text: '', image: 7 } }
  const q = start([long, picture], ids, () => 0)
  assert.deepEqual(
    q.turns.map((t) => t.check),
    [false, false],
  )
})

test('repeats of missed cards are never typed', () => {
  let q = start([card(1)], ids, () => 0)
  q = answer(q, false)
  assert.equal(q.turns[1].check, false)
})

test('a typed answer ignores case, accents, punctuation and spaces', () => {
  assert.ok(typedRight(' Été! ', 'été'))
  assert.ok(typedRight('la fenetre', 'la fenêtre'))
  assert.ok(!typedRight('la fentre', 'la fenêtre'))
  assert.ok(!typedRight('', 'chat'))
})

test('a typed answer may be any one of the listed answers', () => {
  assert.ok(typedRight('de hond', 'de hond, het hondje'))
  assert.ok(!typedRight('hond', 'de hond, het hondje'))
  assert.ok(typedRight('het hondje', 'de hond / het hondje'))
  assert.ok(typedRight('de hond, het hondje', 'de hond, het hondje'))
})

test('timed cards are never picked for the typed check', () => {
  const q = start([{ ...card(1), time_limit_ms: 3000 }], ids, () => 0)
  assert.equal(q.turns[0].check, false)
})

test('right within the limit is known', () => {
  assert.deepEqual(timedVerdict(' 24 ', '24', 2900, 3000, false), { known: true, tone: 'yes', message: '✓ Goed!' })
})

test('right but slow is not known', () => {
  assert.deepEqual(timedVerdict('24', '24', 4200, 3000, false), {
    known: false,
    tone: 'late',
    message: 'Goed, maar nog wat sneller! (4,2 s)',
  })
})

test('wrong is not known', () => {
  assert.deepEqual(timedVerdict('25', '24', 1000, 3000, false), { known: false, tone: 'no', message: 'Het is 24' })
})

test('a right repeat ends the loop however slow', () => {
  assert.deepEqual(timedVerdict('24', '24', 5100, 3000, true), { known: true, tone: 'yes', message: 'Goed! (5,1 s)' })
})

test('only number answers get the number keyboard', () => {
  assert.ok(numeric(' 24 '))
  assert.ok(!numeric('de kat'))
  assert.ok(!numeric('2,5'))
})
