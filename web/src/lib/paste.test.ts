import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parsePaste } from './paste.ts'

const pairs = (text: string, existing: string[] = []) =>
  parsePaste(text, existing).map((p) => [p.front, p.back, p.problem, p.duplicate])

test('a tab wins over " - " and " = "', () => {
  assert.deepEqual(pairs('a - b\tc = d'), [['a - b', 'c = d', null, false]])
})

test('" - " wins over " = ", and only the first one splits', () => {
  assert.deepEqual(pairs('a = b - c - d'), [['a = b', 'c - d', null, false]])
  assert.deepEqual(pairs('le chat = de kat'), [['le chat', 'de kat', null, false]])
})

test('hyphens without spaces do not split', () => {
  assert.deepEqual(pairs("l'arc-en-ciel"), [["l'arc-en-ciel", '', 'split', false]])
})

test('empty sides are flagged', () => {
  assert.deepEqual(pairs('le chat -  \n = de kat'), [
    ['le chat', '', 'empty', false],
    ['', 'de kat', 'empty', false],
  ])
})

test('fronts already in the deck or earlier in the paste are possible duplicates', () => {
  assert.deepEqual(pairs('le chat\tde kat\nle chien\tde hond\nle chien\tde hond', ['le chat']), [
    ['le chat', 'de kat', null, true],
    ['le chien', 'de hond', null, false],
    ['le chien', 'de hond', null, true],
  ])
})

test('blank lines are skipped and Windows line ends are fine', () => {
  const lines = parsePaste('un\teen\r\n\r\n  \r\ndeux\tTwee\r\n', [])
  assert.deepEqual(lines.map((l) => [l.line, l.front, l.back]), [[1, 'un', 'een'], [4, 'deux', 'Twee']])
})

test('sides over 500 characters are flagged', () => {
  assert.equal(parsePaste(`x\t${'é'.repeat(501)}`, [])[0].problem, 'long')
  assert.equal(parsePaste(`x\t${'é'.repeat(500)}`, [])[0].problem, null)
})
