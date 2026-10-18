<script lang="ts">
  import { tick } from 'svelte'
  import { api, ApiError } from '../lib/api'
  import { answer, skip, start, type Queue, type Side, type StudyCard } from '../lib/session'

  let { childId, subject }: { childId: number; subject: number | null } = $props()

  let queue = $state.raw<Queue | null>(null)
  let loadFailed = $state(false)
  let revealed = $state(false)
  let saving = $state(false)
  /** The answer that failed to save; the card stays until it is saved (spec §6.3). */
  let unsaved = $state<boolean | null>(null)
  let stopped = $state(false)
  let buttons = $state<HTMLDivElement>()

  /** Buttons are swapped on every step; put focus on the new first one so keyboard and
   *  switch users don't have to find their way back from the top of the page. */
  async function focusButtons() {
    await tick()
    buttons?.querySelector('button')?.focus()
  }

  function reveal() {
    revealed = true
    focusButtons()
  }

  $effect(() => {
    const query = subject === null ? '' : `?subject=${subject}`
    api<StudyCard[]>(`children/${childId}/session${query}`).then(
      (cards) => (queue = start(cards)),
      () => (loadFailed = true),
    )
  })

  const turn = $derived(queue?.turns[queue.index])
  const finished = $derived(queue !== null && (stopped || turn === undefined))

  async function respond(known: boolean) {
    if (!queue || !turn || !revealed || saving) return
    if (turn.answerId !== null) {
      saving = true
      try {
        await api('answers', {
          id: turn.answerId,
          child_id: childId,
          card_id: turn.card.card_id,
          direction: turn.card.direction,
          known,
        })
      } catch (err) {
        if (err instanceof ApiError && err.status === 404) {
          // Archived or unassigned meanwhile: there is nothing left to save, move on.
          unsaved = null
          revealed = false
          if (queue) queue = skip(queue)
          focusButtons()
          return
        }
        unsaved = known
        return
      } finally {
        saving = false
      }
    }
    unsaved = null
    revealed = false
    queue = answer(queue, known)
    focusButtons()
  }

  function onkeydown(e: KeyboardEvent) {
    // Alt/Cmd/Ctrl+arrow is browser navigation, never an answer.
    if (finished || !turn || e.altKey || e.metaKey || e.ctrlKey) return
    if (e.key === ' ') {
      // Never let space also "click" a focused answer button.
      e.preventDefault()
      if (!e.repeat && !revealed) reveal()
    } else if (!e.repeat && revealed && unsaved === null) {
      if (e.key === 'ArrowLeft') respond(false)
      if (e.key === 'ArrowRight') respond(true)
    }
  }
</script>

<svelte:window {onkeydown} />

{#snippet side(s: Side)}
  {#if s.image !== null}<img class="card-image" src="/api/images/{s.image}" alt="" />{/if}
  {#if s.text}<p class="card-text">{s.text}</p>{/if}
{/snippet}

{#if loadFailed}
  <div class="card narrow">
    <p>De kaartjes konden niet geladen worden.</p>
    <div class="actions"><a class="button" href="#/kind/{childId}">Terug</a></div>
  </div>
{:else if !queue}
  <p class="notice">Laden…</p>
{:else if finished}
  {@const s = queue.summary}
  <div class="card narrow summary">
    {#if s.known + s.notKnown === 0}
      <h1>Niets geoefend</h1>
      <p>Er waren geen kaartjes om te oefenen.</p>
    {:else}
      <h1>Goed gedaan!</h1>
      <ul class="tally">
        <li class="yes"><strong>{s.known}</strong> wist je</li>
        <li class="no"><strong>{s.notKnown}</strong> nog niet</li>
        <li><strong>{s.repeats}</strong> keer herhaald</li>
      </ul>
    {/if}
    <div class="actions"><a class="button big" href="#/">Klaar</a></div>
  </div>
{:else if turn}
  <div class="study">
    <header class="study-bar">
      <span>{turn.card.subject} · {turn.card.deck}</span>
      <span>{queue.index + 1} van {queue.turns.length}</span>
    </header>

    <section class="flashcard" aria-live="polite">
      {@render side(turn.card.question)}
      {#if revealed}
        <hr />
        {@render side(turn.card.answer)}
        {#if turn.card.notes}<p class="notes">{turn.card.notes}</p>{/if}
      {/if}
    </section>

    <div class="answer-buttons" bind:this={buttons}>
      {#if unsaved !== null}
        <p class="error" role="alert">Dat lukte even niet. Probeer het nog eens.</p>
        <button disabled={saving} onclick={() => respond(unsaved!)}>Opnieuw proberen</button>
      {:else if !revealed}
        <button class="reveal" onclick={reveal}>Toon antwoord</button>
      {:else}
        <button class="no" disabled={saving} onclick={() => respond(false)}>Wist ik niet</button>
        <button class="yes" disabled={saving} onclick={() => respond(true)}>Wist ik</button>
      {/if}
    </div>

    <div class="actions">
      <button class="secondary" disabled={saving} onclick={() => (stopped = true)}>
        Stoppen
      </button>
    </div>
  </div>
{/if}
