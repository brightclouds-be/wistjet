<script lang="ts">
  import { tick } from 'svelte'
  import { api, ApiError } from '../lib/api'
  import { answer, skip, start, timedVerdict, typedRight, type Queue, type Side, type StudyCard } from '../lib/session'
  import BoxBars, { type SubjectBoxes } from '../components/BoxBars.svelte'

  let { childId, subject }: { childId: number; subject: number | null } = $props()

  let queue = $state.raw<Queue | null>(null)
  let loadFailed = $state(false)
  let revealed = $state(false)
  let saving = $state(false)
  /** The answer that failed to save; the card stays until it is saved (spec §6.3). */
  let unsaved = $state<boolean | null>(null)
  let stopped = $state(false)
  /** Cards that went to a later box this session, for the summary. */
  let movedUp = $state(0)
  /** On a check turn: what the child typed, and whether it was right once checked. */
  let typed = $state('')
  let typedOk = $state<boolean | null>(null)
  /** On a timed turn (tables spec §7): the result once submitted, and when the sum appeared. */
  let verdict = $state<ReturnType<typeof timedVerdict> | null>(null)
  let shownAt = 0
  let buttons = $state<HTMLDivElement>()

  /** Buttons are swapped on every step; put focus on the new first one so keyboard and
   *  switch users don't have to find their way back from the top of the page. */
  async function focusButtons() {
    await tick()
    buttons?.querySelector<HTMLElement>('input, button')?.focus()
  }

  function reveal() {
    revealed = true
    focusButtons()
  }

  function check(e: SubmitEvent) {
    e.preventDefault()
    if (!turn) return
    const limit = turn.card.time_limit_ms
    if (limit === null) typedOk = typedRight(typed, turn.card.answer.text)
    else verdict = timedVerdict(typed, turn.card.answer.text, performance.now() - shownAt, limit, turn.answerId === null)
    reveal()
  }

  function nextCard() {
    revealed = false
    typed = ''
    typedOk = null
    verdict = null
  }

  $effect(() => {
    const query = subject === null ? '' : `?subject=${subject}`
    api<StudyCard[]>(`children/${childId}/session${query}`).then(
      (cards) => (queue = start(cards)),
      () => (loadFailed = true),
    )
  })

  const turn = $derived(queue?.turns[queue.index])
  const timed = $derived(turn != null && turn.card.time_limit_ms !== null)
  // The clock runs from the moment a sum is on screen; it is never shown.
  $effect(() => {
    if (turn) shownAt = performance.now()
  })
  const finished = $derived(queue !== null && (stopped || turn === undefined))
  /** Where the practised subjects' cards stand now, fetched once the session ends. */
  const progress = $derived.by(() => {
    if (!finished || !queue) return null
    const practised = new Set(queue.turns.slice(0, queue.index).map((t) => t.card.subject))
    return api<{ progress: SubjectBoxes[] }>(`children/${childId}/home`).then((h) =>
      h.progress.filter((s) => practised.has(s.name)),
    )
  })

  async function respond(known: boolean) {
    if (!queue || !turn || !revealed || saving) return
    if (turn.answerId !== null) {
      saving = true
      try {
        const saved = await api<{ moved_up: boolean }>('answers', {
          id: turn.answerId,
          child_id: childId,
          card_id: turn.card.card_id,
          direction: turn.card.direction,
          known,
        })
        if (saved.moved_up) movedUp += 1
      } catch (err) {
        if (err instanceof ApiError && err.status === 404) {
          // Archived or unassigned meanwhile: there is nothing left to save, move on.
          unsaved = null
          nextCard()
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
    nextCard()
    queue = answer(queue, known)
    focusButtons()
  }

  function onkeydown(e: KeyboardEvent) {
    // Alt/Cmd/Ctrl+arrow is browser navigation, never an answer.
    if (finished || !turn || e.altKey || e.metaKey || e.ctrlKey) return
    // Typing the answer, or a typed answer that decides for itself: no shortcuts.
    if (turn.check || turn.card.time_limit_ms !== null) return
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
      {#await progress then subjects}
        {#if subjects?.length}<BoxBars {subjects} />{/if}
      {/await}
      {#if movedUp > 0}
        <p class="moved-up">⬆ {movedUp} {movedUp === 1 ? 'kaartje' : 'kaartjes'} een bakje verder</p>
      {/if}
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
        {#if verdict}
          <p class="typed {verdict.tone}">{verdict.message}</p>
        {:else if typedOk === true}
          <p class="typed yes">✓ Goed getypt!</p>
        {:else if typedOk === false}
          <p class="typed no">Je typte: <strong>{typed.trim() || '(niets)'}</strong></p>
        {/if}
        <hr />
        {@render side(turn.card.answer)}
        {#if turn.card.notes}<p class="notes">{turn.card.notes}</p>{/if}
      {/if}
    </section>

    <div class="answer-buttons" bind:this={buttons}>
      {#if unsaved !== null}
        <p class="error" role="alert">Dat lukte even niet. Probeer het nog eens.</p>
        <button disabled={saving} onclick={() => respond(unsaved!)}>Opnieuw proberen</button>
      {:else if (turn.check || timed) && !revealed}
        <form class="type-answer" onsubmit={check}>
          <label>
            Typ je antwoord
            <input
              bind:value={typed}
              inputmode={timed ? 'numeric' : 'text'}
              autocomplete="off"
              autocapitalize="off"
              spellcheck="false"
            />
          </label>
          <button>Controleer</button>
        </form>
      {:else if !revealed}
        <button class="reveal" onclick={reveal}>Toon antwoord</button>
      {:else if verdict}
        <button class={verdict.known ? 'yes' : 'no'} disabled={saving} onclick={() => respond(verdict!.known)}>
          Verder
        </button>
      {:else if typedOk !== null}
        <button class={typedOk ? 'yes' : 'no'} disabled={saving} onclick={() => respond(typedOk!)}>
          Verder
        </button>
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
