<script lang="ts">
  import Swatches from '../../components/Swatches.svelte'
  import { api } from '../../lib/api'
  import { COLOURS } from '../../lib/colours'
  import { problem } from '../../lib/messages'
  import { initial } from '../../lib/text'
  import type { ChildRow } from '../../lib/types'

  let children = $state<ChildRow[]>([])
  /** Seconds per sum on "Typen op tijd" decks; the server keeps milliseconds. */
  const blank = () => ({ name: '', avatar: COLOURS[0], seconds: 3 })
  let draft = $state(blank())
  /** The child being edited; id 0 = none. */
  let edit = $state({ id: 0, name: '', avatar: '', seconds: 3 })
  const ms = (seconds: number) => Math.round(seconds * 1000)
  let error = $state('')

  async function load() {
    children = await api<ChildRow[]>('parent/children')
  }
  load()

  async function run(action: () => Promise<unknown>): Promise<boolean> {
    error = ''
    try {
      await action()
      await load()
      return true
    } catch (err) {
      error = problem(err)
      return false
    }
  }

  async function add(e: SubmitEvent) {
    e.preventDefault()
    const body = { name: draft.name, avatar: draft.avatar, time_limit_ms: ms(draft.seconds) }
    if (await run(() => api('parent/children', body))) draft = blank()
  }

  async function save(e: SubmitEvent, child: ChildRow) {
    e.preventDefault()
    const body = { name: edit.name, avatar: edit.avatar, archived: child.archived, time_limit_ms: ms(edit.seconds) }
    if (await run(() => api(`parent/children/${child.id}`, body, 'PUT'))) edit.id = 0
  }

  const setArchived = (child: ChildRow, archived: boolean) =>
    run(() => api(`parent/children/${child.id}`, { ...child, archived }, 'PUT'))
</script>

{#snippet timeLimit(c: { seconds: number })}
  <label>
    Tijd per som bij "Typen op tijd" (seconden)
    <input type="number" min="1" max="30" step="0.5" bind:value={c.seconds} required />
  </label>
{/snippet}

{#if error}<p class="error" role="alert">{error}</p>{/if}

<ul class="rows">
  {#each children as child (child.id)}
    <li class="row" class:archived={child.archived}>
      {#if edit.id === child.id}
        <form class="grow" onsubmit={(e) => save(e, child)}>
          <label>Naam <input bind:value={edit.name} required /></label>
          <Swatches bind:value={edit.avatar} name="edit-child" />
          {@render timeLimit(edit)}
          <div class="actions">
            <button type="button" class="secondary" onclick={() => (edit.id = 0)}>Annuleer</button>
            <button type="submit">Bewaar</button>
          </div>
        </form>
      {:else}
        <span class="avatar small" style:background={child.avatar}>{initial(child.name)}</span>
        <span class="grow">{child.name}{#if child.archived}<span class="tag">gearchiveerd</span>{/if}</span>
        <button
          class="secondary"
          onclick={() =>
            (edit = { id: child.id, name: child.name, avatar: child.avatar, seconds: child.time_limit_ms / 1000 })}
        >
          Wijzig
        </button>
        <button class="secondary" onclick={() => setArchived(child, !child.archived)}>
          {child.archived ? 'Zet terug' : 'Archiveer'}
        </button>
      {/if}
    </li>
  {/each}
</ul>

<section class="card">
  <h2>Kind toevoegen</h2>
  <form onsubmit={add}>
    <label>Naam <input bind:value={draft.name} required /></label>
    <Swatches bind:value={draft.avatar} name="new-child" />
    {@render timeLimit(draft)}
    <div class="actions"><button type="submit">Toevoegen</button></div>
  </form>
</section>
