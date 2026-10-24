export type Op = '×' | ':'

/** Tables as pasted lines ("1 × 4 = 4", "8 : 4 = 2"), one per sum, for the paste preview
 *  (tables spec §6): tables ascending, × before :, 1 to 10 within each. */
export function tableLines(tables: number[], ops: Op[]): string {
  const lines: string[] = []
  for (const n of [...tables].sort((a, b) => a - b))
    for (const op of ['×', ':'] as const) {
      if (!ops.includes(op)) continue
      for (let k = 1; k <= 10; k++) lines.push(op === '×' ? `${k} × ${n} = ${k * n}` : `${k * n} : ${n} = ${k}`)
    }
  return lines.join('\n')
}
