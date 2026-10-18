<script lang="ts">
  import CardForm from '../../components/CardForm.svelte'
  import DeckForm from '../../components/DeckForm.svelte'
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import { parsePaste, type Pasted } from '../../lib/paste'
  import type { CardInput, CardRow, ChildProgress, ChildRow, DeckInput, DeckRow, SubjectRow, SystemRow } from '../../lib/types'

  let { id }: { id: number } = $props()

  const blankCard = (): CardInput => ({
    front_text: '',
    front_image: null,
    back_text: '',
    back_image: null,
    notes: '',
    active: true,
  })

  let deck = $state<DeckRow | null>(null)
  let missing = $state(false)
  let subjects = $state<SubjectRow[]>([])
  let systems = $state<SystemRow[]>([])
  let children = $state<ChildRow[]>([])
  let cards = $state<CardRow[]>([])
  let progress = $state<ChildProgress[]>([])

  let settings = $state<DeckInput | null>(null)
  let newCard = $state(blankCard())
  /** The card being edited, as a form copy; `editId` 0 = none. */
  let editId = $state(0)
  let editCard = $state(blankCard())
  let pasteText = $state('')
  let preview = $state<(Pasted & { keep: boolean })[] | null>(null)

  let errors = $state({ cards: '', paste: '', progress: '', settings: '' })
  let saved = $state('')

  async function load() {
    const [decks, s, sy, ch] = await Promise.all([
      api<DeckRow[]>('parent/decks'),
      api<SubjectRow[]>('parent/subjects'),
      api<SystemRow[]>('parent/systems'),
      api<ChildRow[]>('parent/children'),
    ])
    deck = decks.find((d) => d.id === id) ?? null
    missing = deck === null
    if (!deck) return
    subjects = s
    systems = sy
    children = ch
    const { name, subject_id, system_id, direction_mode, children: assigned, archived } = deck
    settings = { name, subject_id, system_id, direction_mode, children: [...assigned], archived }
    ;[cards, progress] = await Promise.all([
      api<CardRow[]>(`parent/decks/${id}/cards`),
      api<ChildProgress[]>(`parent/decks/${id}/progress`),
    ])
  }

  $effect(() => {
    load()
  })

  /** Runs a change, reloads, and reports a failure in the given section. */
  async function run(section: keyof typeof errors, action: () => Promise<unknown>): Promise<boolean> {
    errors[section] = ''
    saved = ''
    try {
      await action()
      await load()
      return true
    } catch (err) {
      errors[section] = problem(err)
      return false
    }
  }

  async function addCard() {
    if (await run('cards', () => api(`parent/decks/${id}/cards`, [newCard]))) newCard = blankCard()
  }

  function startEdit(card: CardRow) {
    const { id: cardId, ...input } = card
    editId = cardId
    editCard = input
  }

  async function saveEdit() {
    if (await run('cards', () => api(`parent/cards/${editId}`, editCard, 'PUT'))) editId = 0
  }

  const setActive = (card: CardRow, active: boolean) => {
    const { id: cardId, ...input } = card
    return run('cards', () => api(`parent/cards/${cardId}`, { ...input, active }, 'PUT'))
  }

  function showPreview() {
    preview = parsePaste(pasteText, cards.map((c) => c.front_text)).map((p) => ({ ...p, keep: p.problem === null }))
  }

  const toAdd = $derived(preview?.filter((p) => p.problem === null && p.keep) ?? [])

  async function addPasted() {
    const batch = toAdd.map((p) => ({ ...blankCard(), front_text: p.front, back_text: p.back }))
    if (await run('paste', () => api(`parent/decks/${id}/cards`, batch))) {
      saved = `${batch.length} kaartjes toegevoegd.`
      pasteText = ''
      preview = null
    }
  }

  function reset(child: ChildProgress | null) {
    const who = child ? child.name : 'iedereen'
    if (!confirm(`Alle kaartjes van dit deck terug naar bakje 1 voor ${who}? De geschiedenis blijft bewaard.`)) return
    run('progress', () => api(`parent/decks/${id}/reset`, { child_id: child?.child_id ?? null }))
  }

  async function saveSettings() {
    if (settings && (await run('settings', () => api(`parent/decks/${id}`, settings, 'PUT')))) saved = 'Bewaard.'
  }

  const setArchived = (archived: boolean) =>
    settings && run('settings', () => api(`parent/decks/${id}`, { ...settings, archived }, 'PUT'))

  const PROBLEM = { split: 'geen scheiding gevonden', empty: 'voor- of achterkant leeg', long: 'langer dan 500 tekens' }
</script>

{#snippet side(text: string, image: number | null)}
  <span class="card-side">
    {#if image !== null}<img class="thumb" src="/api/images/{image}" alt="" />{/if}
    {text}
  </span>
{/snippet}

<p><a href="#/ouder/decks">← Alle decks</a></p>

{#if missing}
  <p class="card">Dit deck bestaat niet.</p>
{:else if deck && settings}
  <h2 class="deck-title">{deck.name}{#if deck.archived}<em class="muted"> · gearchiveerd</em>{/if}</h2>
  {#if saved}<p class="saved" role="status">{saved}</p>{/if}

  <section class="card">
    <h3>Kaartjes ({cards.filter((c) => c.active).length})</h3>
    <ul class="rows">
      {#each cards as card (card.id)}
        <li class="row" class:archived={!card.active}>
          {#if editId === card.id}
            <div class="grow">
              <CardForm bind:card={editCard} submitLabel="Bewaar" onsubmit={saveEdit} oncancel={() => (editId = 0)} />
            </div>
          {:else}
            <span class="grow card-line">
              {@render side(card.front_text, card.front_image)}
              <span class="muted">→</span>
              {@render side(card.back_text, card.back_image)}
              {#if card.notes}<span class="muted note">{card.notes}</span>{/if}
              {#if !card.active}<em> · uit</em>{/if}
            </span>
            <button class="secondary" onclick={() => startEdit(card)}>Wijzig</button>
            <button class="secondary" onclick={() => setActive(card, !card.active)}>
              {card.active ? 'Zet uit' : 'Zet aan'}
            </button>
          {/if}
        </li>
      {:else}
        <li class="muted">Nog geen kaartjes.</li>
      {/each}
    </ul>
    <h3>Kaartje toevoegen</h3>
    <CardForm bind:card={newCard} submitLabel="Toevoegen" onsubmit={addCard} />
    {#if errors.cards}<p class="error" role="alert">{errors.cards}</p>{/if}
  </section>

  <section class="card">
    <h3>Lijst plakken</h3>
    <p class="muted">
      Eén kaartje per regel: voorkant en achterkant gescheiden door een tab, " - " of " = ".
      Afbeeldingen voeg je daarna per kaartje toe.
    </p>
    <textarea bind:value={pasteText} rows="6" aria-label="Geplakte lijst" placeholder={'le chat - de kat\nle chien - de hond'}></textarea>
    <div class="actions"><button class="secondary" onclick={showPreview} disabled={!pasteText.trim()}>Bekijk</button></div>
    {#if preview}
      <table class="preview">
        <thead><tr><th></th><th>Voorkant</th><th>Achterkant</th><th></th></tr></thead>
        <tbody>
          {#each preview as row (row.line)}
            <tr class:skipped={row.problem !== null}>
              <td>
                {#if row.problem === null}
                  <input type="checkbox" bind:checked={row.keep} aria-label="Regel {row.line} toevoegen" />
                {/if}
              </td>
              <td>{row.front}</td>
              <td>{row.back}</td>
              <td class="muted">
                {#if row.problem}regel {row.line}: {PROBLEM[row.problem]}{:else if row.duplicate}staat er misschien al in{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="actions">
        <button onclick={addPasted} disabled={toAdd.length === 0}>Voeg {toAdd.length} kaartjes toe</button>
      </div>
    {/if}
    {#if errors.paste}<p class="error" role="alert">{errors.paste}</p>{/if}
  </section>

  <section class="card">
    <h3>Voortgang</h3>
    {#if progress.length === 0}
      <p class="muted">Niemand oefent dit deck.</p>
    {:else}
      <table class="progress-table">
        <thead>
          <tr>
            <th>Kind</th><th>Vandaag</th>
            {#each progress[0].boxes as _, i (i)}<th>Bakje {i + 1}</th>{/each}
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each progress as child (child.child_id)}
            <tr>
              <td>{child.name}</td>
              <td>{child.due}</td>
              {#each child.boxes as count, i (i)}<td>{count}</td>{/each}
              <td><button class="secondary small" onclick={() => reset(child)}>Opnieuw beginnen</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="actions">
        <button class="secondary" onclick={() => reset(null)}>Opnieuw beginnen voor iedereen</button>
      </div>
    {/if}
    {#if errors.progress}<p class="error" role="alert">{errors.progress}</p>{/if}
  </section>

  <section class="card">
    <h3>Instellingen</h3>
    <DeckForm bind:deck={settings} {subjects} {systems} {children} submitLabel="Bewaar" onsubmit={saveSettings} />
    {#if errors.settings}<p class="error" role="alert">{errors.settings}</p>{/if}
    <div class="actions">
      <button class="secondary" onclick={() => setArchived(!deck!.archived)}>
        {deck.archived ? 'Zet terug' : 'Archiveer dit deck'}
      </button>
    </div>
  </section>
{:else}
  <p class="notice">Laden…</p>
{/if}
