import { ApiError } from './api'

const FIELD: Record<string, string> = {
  name: 'Geef een naam van hoogstens 50 tekens.',
  avatar: 'Kies een kleur.',
  subject_id: 'Kies een vak.',
  system_id: 'Kies een studiesysteem.',
  direction_mode: 'Kies een richting.',
  children: 'Een van de gekozen kinderen bestaat niet meer.',
  front: 'De voorkant heeft tekst (hoogstens 500 tekens) of een afbeelding nodig.',
  back: 'De achterkant heeft tekst (hoogstens 500 tekens) of een afbeelding nodig.',
  notes: 'Notities zijn hoogstens 1000 tekens.',
  front_image: 'Die afbeelding bestaat niet meer.',
  back_image: 'Die afbeelding bestaat niet meer.',
  image_type: 'Kies een JPEG-, PNG-, WebP- of GIF-afbeelding.',
  image_size: 'Een afbeelding is hoogstens 5 MB.',
  child_id: 'Dat kind oefent dit deck niet.',
}

/** A Dutch sentence for a failed parent action. */
export function problem(err: unknown): string {
  if (err instanceof ApiError) {
    const field = err.body?.field
    if (field && field in FIELD) return FIELD[field]
    if (err.status === 404) return 'Dit bestaat niet meer. Laad de pagina opnieuw.'
  }
  return 'Opslaan lukte niet. Probeer het opnieuw.'
}
