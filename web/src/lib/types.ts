/** Shapes of the parent API (see `src/manage.rs`). */
export type Direction = 'front_to_back' | 'back_to_front' | 'both'
export type ChildRow = { id: number; name: string; avatar: string; archived: boolean; time_limit_ms: number }
export type SubjectRow = { id: number; name: string; archived: boolean }
export type BoxRow = { name: string; interval_days: number; success: number; failure: number }
export type SystemDetail = {
  id: number
  name: string
  preset: boolean
  repeat_missed: boolean
  boxes: BoxRow[]
  decks: number
  is_default: boolean
}
export type BoxInput = BoxRow & { origin: number | null }
export type SystemInput = { name: string; repeat_missed: boolean; boxes: BoxInput[]; previous: BoxRow[] | null }
export type DeckInput = {
  name: string
  subject_id: number
  system_id: number
  direction_mode: Direction
  answer_mode: 'self' | 'timed'
  children: number[]
  archived: boolean
}
export type DeckRow = DeckInput & { id: number; cards: number }
export type CardInput = {
  front_text: string
  front_image: number | null
  back_text: string
  back_image: number | null
  notes: string
  active: boolean
}
export type CardRow = CardInput & { id: number }
export type ChildProgress = { child_id: number; name: string; due: number; boxes: number[] }
export type ChildOverview = {
  child_id: number
  name: string
  avatar: string
  due: number
  reviewed: number
  known: number
  not_known: number
  rushed: number
  self_known: number
  boxes: number[]
}
export type Settings = { name: string; timezone: string; default_system_id: number }
export type BackupStatus = { dir: string | null; keep: number; latest: string | null }
