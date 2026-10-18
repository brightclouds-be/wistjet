/** Shapes of the parent API (see `src/manage.rs`). */
export type Direction = 'front_to_back' | 'back_to_front' | 'both'
export type ChildRow = { id: number; name: string; avatar: string; archived: boolean }
export type SubjectRow = { id: number; name: string; archived: boolean }
export type SystemRow = { id: number; name: string; preset: boolean; boxes: number }
export type DeckInput = {
  name: string
  subject_id: number
  system_id: number
  direction_mode: Direction
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
