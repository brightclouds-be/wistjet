/** One non-blank pasted line (spec §7.5). Lines with a `problem` are not saved;
 *  possible duplicates are, unless the parent unticks them. */
export type Pasted = {
  line: number
  front: string
  back: string
  problem: 'split' | 'empty' | 'long' | null
  duplicate: boolean
}

// In order of preference: the first separator kind found in a line splits it once.
const SEPARATORS = ['\t', ' - ', ' = ']

export function parsePaste(text: string, existingFronts: string[]): Pasted[] {
  const seen = new Set(existingFronts.map((f) => f.trim()))
  const out: Pasted[] = []
  text.split(/\r?\n/).forEach((raw, i) => {
    if (!raw.trim()) return
    const sep = SEPARATORS.find((s) => raw.includes(s))
    if (!sep) {
      out.push({ line: i + 1, front: raw.trim(), back: '', problem: 'split', duplicate: false })
      return
    }
    const at = raw.indexOf(sep)
    const front = raw.slice(0, at).trim()
    const back = raw.slice(at + sep.length).trim()
    const long = [...front].length > 500 || [...back].length > 500
    const problem = !front || !back ? 'empty' : long ? 'long' : null
    const duplicate = problem === null && seen.has(front)
    if (problem === null) seen.add(front)
    out.push({ line: i + 1, front, back, problem, duplicate })
  })
  return out
}
