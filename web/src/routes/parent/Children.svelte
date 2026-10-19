<script lang="ts">
  import Swatches from '../../components/Swatches.svelte'
  import { api } from '../../lib/api'
  import { COLOURS } from '../../lib/colours'
  import { problem } from '../../lib/messages'
  import { initial } from '../../lib/text'
  import type { ChildRow } from '../../lib/types'

  let children = $state<ChildRow[]>([])
  let draft = $state({ name: '', avatar: COLOURS[0] })
  /** The child being edited; id 0 = none. */
  let edit = $state({ id: 0, name: '', avatar: '' })
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
    if (await run(() => api('parent/children', draft))) draft = { name: '', avatar: COLOURS[0] }
  }

  async function save(e: SubmitEvent, child: ChildRow) {
    e.preventDefault()
    const body = { name: edit.name, avatar: edit.avatar, archived: child.archived }
    if (await run(() => api(`parent/children/${child.id}`, body, 'PUT'))) edit.id = 0
  }

  const setArchived = (child: ChildRow, archived: boolean) =>
    run(() => api(`parent/children/${child.id}`, { ...child, archived }, 'PUT'))
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

<ul class="rows">
  {#each children as child (child.id)}
    <li class="row" class:archived={child.archived}>
      {#if edit.id === child.id}
        <form class="grow" onsubmit={(e) => save(e, child)}>
          <label>Naam <input bind:value={edit.name} required /></label>
          <Swatches bind:value={edit.avatar} name="edit-child" />
          <div class="actions">
            <button type="button" class="secondary" onclick={() => (edit.id = 0)}>Annuleer</button>
            <button type="submit">Bewaar</button>
          </div>
        </form>
      {:else}
        <span class="avatar small" style:background={child.avatar}>{initial(child.name)}</span>
        <span class="grow">{child.name}{#if child.archived}<span class="tag">gearchiveerd</span>{/if}</span>
        <button class="secondary" onclick={() => (edit = { id: child.id, name: child.name, avatar: child.avatar })}>
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
    <div class="actions"><button type="submit">Toevoegen</button></div>
  </form>
</section>
