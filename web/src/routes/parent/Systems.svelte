<script lang="ts">
  import SystemEditor from '../../components/SystemEditor.svelte'
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import { fromSystem, toInput, type Editor } from '../../lib/systemEditor'
  import type { Settings, SystemDetail } from '../../lib/types'

  let systems = $state<SystemDetail[]>([])
  /** The system being edited (`id`), or a new copy (`id` 0). */
  let editing = $state<{ id: number; editor: Editor } | null>(null)
  let error = $state('')
  let busy = $state(false)

  async function load() {
    systems = await api<SystemDetail[]>('parent/systems')
  }
  load().catch(() => (error = 'De studiesystemen konden niet geladen worden.'))

  async function run(action: () => Promise<unknown>): Promise<boolean> {
    if (busy) return false
    busy = true
    error = ''
    try {
      await action()
      await load()
      return true
    } catch (err) {
      error = problem(err)
      return false
    } finally {
      busy = false
    }
  }

  async function save() {
    if (!editing) return
    const { id, editor } = editing
    const input = toInput(editor)
    const action = () => (id ? api(`parent/systems/${id}`, input, 'PUT') : api('parent/systems', input))
    if (await run(action)) editing = null
  }

  function remove(s: SystemDetail) {
    if (confirm(`Studiesysteem "${s.name}" verwijderen?`)) run(() => api(`parent/systems/${s.id}`, {}, 'DELETE'))
  }

  function makeDefault(s: SystemDetail) {
    run(async () => {
      const settings = await api<Settings>('parent/settings')
      await api('parent/settings', { ...settings, default_system_id: s.id }, 'PUT')
    })
  }

  const days = (n: number) => `${n} ${n === 1 ? 'dag' : 'dagen'}`
  const used = (id: number) => systems.find((s) => s.id === id)?.decks ?? 0
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}

{#if editing}
  <section class="card">
    <h2>{editing.id ? 'Studiesysteem wijzigen' : 'Nieuw studiesysteem'}</h2>
    {#if editing.id && used(editing.id) > 0}
      <p class="muted">
        Wijzigingen gelden voor volgende antwoorden; de geplande dagen blijven staan. Kaartjes in
        een weggehaald bakje gaan naar het bakje dat nu op die plaats staat.
      </p>
    {/if}
    <SystemEditor bind:editor={editing.editor} {busy} onsave={save} oncancel={() => (editing = null)} />
  </section>
{:else}
  <ul class="rows">
    {#each systems as s (s.id)}
      <li class="row">
        <span class="grow">
          <strong>{s.name}</strong>{#if s.preset}<span class="tag">vast</span>{/if}{#if s.is_default}<span class="tag">standaard</span>{/if}
          <span class="muted block">
            {s.boxes.map((b) => days(b.interval_days)).join(' → ')} · {s.decks} {s.decks === 1 ? 'deck' : 'decks'}
          </span>
        </span>
        {#if !s.is_default}<button class="secondary" onclick={() => makeDefault(s)}>Maak standaard</button>{/if}
        <button class="secondary" onclick={() => (editing = { id: 0, editor: fromSystem(s, true) })}>Kopieer</button>
        {#if !s.preset}
          <button class="secondary" onclick={() => (editing = { id: s.id, editor: fromSystem(s) })}>Wijzig</button>
          <button class="secondary" onclick={() => remove(s)}>Verwijder</button>
        {/if}
      </li>
    {/each}
  </ul>
  <p class="muted">
    Het vaste systeem volgt de school en kan niet gewijzigd worden. Kopieer het om je eigen
    bakjes en dagen te kiezen. Nieuwe decks krijgen het standaardsysteem.
  </p>
{/if}
