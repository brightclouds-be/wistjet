import type { BoxRow, SystemDetail, SystemInput } from './types'

/** A box in the editor. `success` and `failure` point at other boxes' `key`s, so links
 *  survive reordering; positions are only worked out when saving. `origin` is the box's
 *  position when loaded (`null` for new boxes and copies), which the server uses to move
 *  progress along (spec §5.6). */
export type EditBox = {
  key: number
  origin: number | null
  name: string
  interval_days: number
  success: number
  failure: number
}

export type Editor = {
  name: string
  repeat_missed: boolean
  boxes: EditBox[]
  nextKey: number
  /** What a box removal changed, shown before saving (spec §7.6). */
  notices: string[]
  /** The boxes as loaded (`null` for a copy); the server refuses the save if another tab
   *  changed them meanwhile, because the origins would no longer be true. */
  previous: BoxRow[] | null
}

export function fromSystem(s: SystemDetail, copy = false): Editor {
  return {
    name: copy ? `Kopie van ${s.name}`.slice(0, 50) : s.name,
    repeat_missed: s.repeat_missed,
    boxes: s.boxes.map((b, i) => ({ key: i, origin: copy ? null : i, ...b })),
    nextKey: s.boxes.length,
    notices: [],
    previous: copy ? null : s.boxes,
  }
}

export function toInput(e: Editor): SystemInput {
  const pos = (key: number) => e.boxes.findIndex((b) => b.key === key)
  return {
    name: e.name,
    repeat_missed: e.repeat_missed,
    boxes: e.boxes.map((b) => ({
      name: b.name,
      interval_days: b.interval_days,
      success: pos(b.success),
      failure: pos(b.failure),
      origin: b.origin,
    })),
    previous: e.previous,
  }
}

/** A new last box: double the last interval, known stays here, missed goes to the start. */
export function addBox(e: Editor): Editor {
  const last = e.boxes[e.boxes.length - 1]
  const box: EditBox = {
    key: e.nextKey,
    origin: null,
    name: `Bakje ${e.boxes.length + 1}`,
    interval_days: Math.min(365, (last?.interval_days ?? 1) * 2),
    success: e.nextKey,
    failure: e.boxes[0]?.key ?? e.nextKey,
  }
  return { ...e, boxes: [...e.boxes, box], nextKey: e.nextKey + 1 }
}

/** Removes a box (never the last one left). Links to it reset to the defaults of spec §7.6:
 *  "Wist ik" → the next box, or the box itself when it is last; "Wist ik niet" → the first box. */
export function removeBox(e: Editor, key: number): Editor {
  if (e.boxes.length === 1 || !e.boxes.some((b) => b.key === key)) return e
  const boxes = e.boxes.filter((b) => b.key !== key)
  const notices = [...e.notices]
  const fixed = boxes.map((b, i) => {
    let { success, failure } = b
    if (success === key) {
      const next = boxes[i + 1] ?? b
      success = next.key
      notices.push(`"${b.name}" bij Wist ik gaat nu naar "${next.name}".`)
    }
    if (failure === key) {
      failure = boxes[0].key
      notices.push(`"${b.name}" bij Wist ik niet gaat nu naar "${boxes[0].name}".`)
    }
    return { ...b, success, failure }
  })
  return { ...e, boxes: fixed, notices }
}

/** Moves a box one place up (-1) or down (1); its links move with it. */
export function moveBox(e: Editor, key: number, dir: -1 | 1): Editor {
  const i = e.boxes.findIndex((b) => b.key === key)
  const j = i + dir
  if (i < 0 || j < 0 || j >= e.boxes.length) return e
  const boxes = [...e.boxes]
  ;[boxes[i], boxes[j]] = [boxes[j], boxes[i]]
  return { ...e, boxes }
}
