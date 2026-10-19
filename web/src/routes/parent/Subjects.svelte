<script lang="ts">
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import type { SubjectRow } from '../../lib/types'

  let subjects = $state<SubjectRow[]>([])
  let draft = $state('')
  /** The subject being renamed; id 0 = none. */
  let edit = $state({ id: 0, name: '' })
  let error = $state('')

  async function load() {
    subjects = await api<SubjectRow[]>('parent/subjects')
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
    if (await run(() => api('parent/subjects', { name: draft }))) draft = ''
  }

  async function save(e: SubmitEvent, subject: SubjectRow) {
    e.preventDefault()
    const body = { name: edit.name, archived: subject.archived }
    if (await run(() => api(`parent/subjects/${subject.id}`, body, 'PUT'))) edit.id = 0
  }

  const setArchived = (subject: SubjectRow, archived: boolean) =>
    run(() => api(`parent/subjects/${subject.id}`, { ...subject, archived }, 'PUT'))
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

<ul class="rows">
  {#each subjects as subject (subject.id)}
    <li class="row" class:archived={subject.archived}>
      {#if edit.id === subject.id}
        <form class="grow inline" onsubmit={(e) => save(e, subject)}>
          <input bind:value={edit.name} aria-label="Naam van het vak" required />
          <button type="button" class="secondary" onclick={() => (edit.id = 0)}>Annuleer</button>
          <button type="submit">Bewaar</button>
        </form>
      {:else}
        <span class="grow">{subject.name}{#if subject.archived}<span class="tag">gearchiveerd</span>{/if}</span>
        <button class="secondary" onclick={() => (edit = { id: subject.id, name: subject.name })}>Wijzig</button>
        <button class="secondary" onclick={() => setArchived(subject, !subject.archived)}>
          {subject.archived ? 'Zet terug' : 'Archiveer'}
        </button>
      {/if}
    </li>
  {:else}
    <li class="muted">Nog geen vakken.</li>
  {/each}
</ul>

<section class="card">
  <h2>Vak toevoegen</h2>
  <form class="inline" onsubmit={add}>
    <input bind:value={draft} aria-label="Naam van het vak" placeholder="Bijvoorbeeld Frans" required />
    <button type="submit">Toevoegen</button>
  </form>
</section>
