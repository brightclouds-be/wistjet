import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parsePaste } from './paste.ts'
import { tableLines } from './tables.ts'

test('the table of 4 gives ten products and ten quotients', () => {
  const lines = tableLines([4], ['×', ':']).split('\n')
  assert.equal(lines.length, 20)
  assert.equal(lines[0], '1 × 4 = 4')
  assert.equal(lines[9], '10 × 4 = 40')
  assert.equal(lines[10], '4 : 4 = 1')
  assert.equal(lines[19], '40 : 4 = 10')
})

test('tables come in ascending order, whatever order they were ticked', () => {
  const lines = tableLines([3, 2], ['×']).split('\n')
  assert.deepEqual([lines[0], lines[10]], ['1 × 2 = 2', '1 × 3 = 3'])
})

test('table lines paste as cards', () => {
  assert.deepEqual(parsePaste(tableLines([3], ['×']), [])[6], {
    line: 7,
    front: '7 × 3',
    back: '21',
    problem: null,
    duplicate: false,
  })
})
