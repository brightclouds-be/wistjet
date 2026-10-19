import assert from 'node:assert/strict'
import { test } from 'node:test'
import { addBox, fromSystem, moveBox, removeBox, toInput } from './systemEditor.ts'
import type { SystemDetail } from './types.ts'

const SCHOOL: SystemDetail = {
  id: 1,
  name: 'School 4-box',
  preset: true,
  repeat_missed: true,
  decks: 1,
  is_default: true,
  boxes: [
    { name: 'Box 1', interval_days: 1, success: 1, failure: 0 },
    { name: 'Box 2', interval_days: 2, success: 2, failure: 0 },
    { name: 'Box 3', interval_days: 7, success: 3, failure: 0 },
    { name: 'Box 4', interval_days: 14, success: 3, failure: 0 },
  ],
}

test('a loaded system saves back unchanged', () => {
  const input = toInput(fromSystem(SCHOOL))
  assert.deepEqual(
    input.boxes.map((b) => [b.success, b.failure, b.origin]),
    [[1, 0, 0], [2, 0, 1], [3, 0, 2], [3, 0, 3]],
  )
})

test('an edit sends the boxes it was loaded with, so a stale page can be refused', () => {
  let e = fromSystem(SCHOOL)
  e = removeBox(e, e.boxes[2].key)
  assert.deepEqual(toInput(e).previous, SCHOOL.boxes)
})

test('a copy has no origins, no previous boxes and a new name', () => {
  const e = fromSystem(SCHOOL, true)
  assert.equal(e.name, 'Kopie van School 4-box')
  assert.ok(toInput(e).boxes.every((b) => b.origin === null))
  assert.equal(toInput(e).previous, null)
})

test('removing a box resets links to it and says so', () => {
  let e = fromSystem(SCHOOL)
  e = removeBox(e, e.boxes[2].key) // Box 3; Box 2 → Box 3 on success
  const input = toInput(e)
  assert.deepEqual(input.boxes.map((b) => b.name), ['Box 1', 'Box 2', 'Box 4'])
  // Box 2's success pointed to Box 3: now its next box (Box 4).
  assert.deepEqual(input.boxes.map((b) => [b.success, b.failure, b.origin]), [[1, 0, 0], [2, 0, 1], [2, 0, 3]])
  assert.deepEqual(e.notices, ['"Box 2" bij Wist ik gaat nu naar "Box 4".'])
})

test('a removed success target of the last box becomes the box itself', () => {
  let e = fromSystem(SCHOOL)
  const last = e.boxes[3]
  e = { ...e, boxes: e.boxes.map((b) => (b.key === last.key ? { ...b, success: e.boxes[2].key } : b)) }
  e = removeBox(e, e.boxes[2].key)
  assert.equal(toInput(e).boxes[2].success, 2)
})

test('a removed failure target becomes the first box', () => {
  let e = fromSystem(SCHOOL)
  e = { ...e, boxes: e.boxes.map((b, i) => (i === 3 ? { ...b, failure: e.boxes[1].key } : b)) }
  e = removeBox(e, e.boxes[1].key)
  assert.equal(toInput(e).boxes[2].failure, 0)
  assert.ok(e.notices.some((n) => n.includes('Wist ik niet')))
})

test('moving boxes keeps their links', () => {
  let e = fromSystem(SCHOOL)
  e = moveBox(e, e.boxes[1].key, -1) // Box 2 to the top
  const input = toInput(e)
  assert.deepEqual(input.boxes.map((b) => b.name), ['Box 2', 'Box 1', 'Box 3', 'Box 4'])
  // Box 2 still goes to Box 3 (now position 2) and back to Box 1 (now position 1).
  assert.deepEqual([input.boxes[0].success, input.boxes[0].failure], [2, 1])
  assert.deepEqual(input.boxes.map((b) => b.origin), [1, 0, 2, 3])
})

test('moving past either end changes nothing', () => {
  const e = fromSystem(SCHOOL)
  assert.equal(moveBox(e, e.boxes[0].key, -1), e)
  assert.equal(moveBox(e, e.boxes[3].key, 1), e)
})

test('a new box goes last, keeps cards that are known, and sends missed ones to the start', () => {
  let e = addBox(fromSystem(SCHOOL))
  const input = toInput(e)
  assert.equal(input.boxes.length, 5)
  assert.deepEqual(input.boxes[4], { name: 'Bakje 5', interval_days: 28, success: 4, failure: 0, origin: null })
})
