<script lang="ts">
  import type { ChildRow, DeckInput, SubjectRow, SystemDetail } from '../lib/types'

  let {
    deck = $bindable(),
    subjects,
    systems,
    children,
    submitLabel,
    onsubmit,
  }: {
    deck: DeckInput
    subjects: SubjectRow[]
    systems: SystemDetail[]
    children: ChildRow[]
    submitLabel: string
    onsubmit: () => void
  } = $props()

  const DIRECTIONS = [
    ['front_to_back', 'Voorkant → achterkant'],
    ['back_to_front', 'Achterkant → voorkant'],
    ['both', 'Beide richtingen (elk apart geoefend)'],
  ] as const

  // Archived subjects and children are only offered when this deck already uses them.
  const subjectOptions = $derived(subjects.filter((s) => !s.archived || s.id === deck.subject_id))
  const childOptions = $derived(children.filter((c) => !c.archived || deck.children.includes(c.id)))

  function submit(e: SubmitEvent) {
    e.preventDefault()
    // A timed deck asks the sum and wants the result (tables spec §5).
    if (deck.answer_mode === 'timed') deck.direction_mode = 'front_to_back'
    onsubmit()
  }
</script>

<form onsubmit={submit}>
  <label>Naam <input bind:value={deck.name} required /></label>
  <label>
    Vak
    <select bind:value={deck.subject_id} required>
      {#each subjectOptions as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
    </select>
  </label>
  <label>
    Studiesysteem
    <select bind:value={deck.system_id}>
      {#each systems as s (s.id)}<option value={s.id}>{s.name} ({s.boxes.length} bakjes)</option>{/each}
    </select>
  </label>
  <fieldset class="choices">
    <legend>Antwoorden</legend>
    <label class="choice"><input type="radio" name="answer" value="self" bind:group={deck.answer_mode} /> Zelf beoordelen</label>
    <label class="choice">
      <input type="radio" name="answer" value="timed" bind:group={deck.answer_mode} /> Typen op tijd (bv. tafels)
    </label>
  </fieldset>
  {#if deck.answer_mode === 'self'}
    <fieldset class="choices">
      <legend>Richting</legend>
      {#each DIRECTIONS as [value, label] (value)}
        <label class="choice"><input type="radio" name="direction" {value} bind:group={deck.direction_mode} /> {label}</label>
      {/each}
    </fieldset>
  {/if}
  <fieldset class="choices">
    <legend>Wie oefent dit deck?</legend>
    {#each childOptions as c (c.id)}
      <label class="choice"><input type="checkbox" value={c.id} bind:group={deck.children} /> {c.name}</label>
    {:else}
      <p class="muted">Nog geen kinderen.</p>
    {/each}
  </fieldset>
  <div class="actions"><button type="submit">{submitLabel}</button></div>
</form>
