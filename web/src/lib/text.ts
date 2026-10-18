/** First visible character of a name, keeping emoji and flags whole. */
export function initial(name: string): string {
  const first = new Intl.Segmenter().segment(name)[Symbol.iterator]().next()
  return first.done ? '' : first.value.segment
}
