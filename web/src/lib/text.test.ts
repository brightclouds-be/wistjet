import assert from 'node:assert/strict'
import { test } from 'node:test'
import { initial } from './text.ts'

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
