<script lang="ts">
  import DeckForm from '../../components/DeckForm.svelte'
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import type { ChildRow, DeckInput, DeckRow, SubjectRow, SystemDetail } from '../../lib/types'

  let decks = $state<DeckRow[]>([])
  let subjects = $state<SubjectRow[]>([])
  let systems = $state<SystemDetail[]>([])
  let children = $state<ChildRow[]>([])
  let draft = $state<DeckInput | null>(null)
  let error = $state('')

  async function load() {
    ;[decks, subjects, systems, children] = await Promise.all([
      api<DeckRow[]>('parent/decks'),
      api<SubjectRow[]>('parent/subjects'),
      api<SystemDetail[]>('parent/systems'),
      api<ChildRow[]>('parent/children'),
    ])
    const subject = subjects.find((s) => !s.archived)
    draft = subject
      ? {
          name: '',
          subject_id: subject.id,
          system_id: (systems.find((s) => s.is_default) ?? systems[0])?.id ?? 1,
          direction_mode: 'front_to_back',
          children: children.filter((c) => !c.archived).map((c) => c.id),
          archived: false,
        }
      : null
  }
  load()

  async function add() {
    if (!draft) return
    error = ''
    try {
      const { id } = await api<{ id: number }>('parent/decks', draft)
      location.hash = `#/ouder/decks/${id}`
    } catch (err) {
      error = problem(err)
    }
  }

  const names = (ids: number[]) =>
    children.filter((c) => ids.includes(c.id)).map((c) => c.name).join(', ') || 'niemand'
</script>

{#each subjects as subject (subject.id)}
  {@const own = decks.filter((d) => d.subject_id === subject.id)}
  {#if own.length > 0}
    <h2>{subject.name}{#if subject.archived}<em class="muted"> · gearchiveerd</em>{/if}</h2>
    <ul class="rows">
      {#each own as deck (deck.id)}
        <li class="row" class:archived={deck.archived}>
          <a class="grow deck-link" href="#/ouder/decks/{deck.id}">
            <strong>{deck.name}</strong>{#if deck.archived}<em> · gearchiveerd</em>{/if}
            <span class="muted">{deck.cards} kaartjes · {names(deck.children)}</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
{/each}

<section class="card">
  <h2>Deck toevoegen</h2>
  {#if draft}
    <DeckForm bind:deck={draft} {subjects} {systems} {children} submitLabel="Toevoegen" onsubmit={add} />
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  {:else}
    <p>Maak eerst een vak aan bij <a href="#/ouder/vakken">Vakken</a>.</p>
  {/if}
</section>
