import assert from 'node:assert/strict'
import { test } from 'node:test'
import { howOften, initial, whenNext } from './text.ts'

test('initial is the first letter', () => {
  assert.equal(initial('Lotte'), 'L')
})

test('initial keeps an emoji whole', () => {
  assert.equal(initial('🦊 Lotte'), '🦊')
  assert.equal(initial('👩‍👧 Mama'), '👩‍👧')
  assert.equal(initial('🇧🇪 Wout'), '🇧🇪')
})

test('initial of an empty name is empty', () => {
  assert.equal(initial(''), '')
})

test('next cards tomorrow say "morgen"', () => {
  assert.equal(whenNext('2026-10-19', new Date(2026, 9, 18, 21, 30)), 'morgen')
})

test('later next cards name the day', () => {
  assert.equal(whenNext('2026-10-24', new Date(2026, 9, 18)), 'op zaterdag 24 oktober')
})

test('boxes are named by how often their cards come back', () => {
  assert.equal(howOften(1), 'elke dag')
  assert.equal(howOften(2), 'om de 2 dagen')
  assert.equal(howOften(7), 'elke week')
  assert.equal(howOften(14), 'om de 2 weken')
  assert.equal(howOften(10), 'om de 10 dagen')
})
