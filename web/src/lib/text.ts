/** First visible character of a name, keeping emoji and flags whole. */
export function initial(name: string): string {
  const first = new Intl.Segmenter().segment(name)[Symbol.iterator]().next()
  return first.done ? '' : first.value.segment
}

/** When the next cards come up, as the end of "De volgende kaartjes komen …". */
export function whenNext(date: string, today: Date = new Date()): string {
  const [y, m, d] = date.split('-').map(Number)
  const next = new Date(y, m - 1, d)
  const tomorrow = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 1)
  if (next.getTime() === tomorrow.getTime()) return 'morgen'
  return `op ${next.toLocaleDateString('nl-BE', { weekday: 'long', day: 'numeric', month: 'long' })}`
}
