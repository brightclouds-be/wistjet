<script lang="ts">
  import { api, ApiError } from '../lib/api'
  import Children from './parent/Children.svelte'
  import Deck from './parent/Deck.svelte'
  import Decks from './parent/Decks.svelte'
  import Overview from './parent/Overview.svelte'
  import Settings from './parent/Settings.svelte'
  import Subjects from './parent/Subjects.svelte'
  import Systems from './parent/Systems.svelte'

  let { route }: { route: string } = $props()

  // #/ouder (overzicht), #/ouder/<tab>, #/ouder/decks/<id>
  const section = $derived(route.match(/^#\/ouder\/(overzicht|kinderen|vakken|decks|systemen|instellingen)(?:\/(\d+))?/))
  const TABS = [
    ['overzicht', 'Overzicht'],
    ['kinderen', 'Kinderen'],
    ['vakken', 'Vakken'],
    ['decks', 'Decks'],
    ['systemen', 'Studiesystemen'],
    ['instellingen', 'Instellingen'],
  ] as const

  const IDLE_MS = 10 * 60 * 1000

  let unlocked = $state<boolean | null>(null)
  let pin = $state('')
  let message = $state('')

  api('parent/session').then(
    () => (unlocked = true),
    () => (unlocked = false),
  )

  async function unlock(e: SubmitEvent) {
    e.preventDefault()
    message = ''
    try {
      await api('parent/login', { pin })
      unlocked = true
    } catch (err) {
      if (err instanceof ApiError && err.status === 429) {
        message = `Te veel pogingen. Probeer opnieuw over ${err.body?.retry_after ?? 60} seconden.`
      } else if (err instanceof ApiError && err.status === 401) {
        message = 'Die pincode klopt niet.'
      } else {
        message = 'Er ging iets mis. Probeer het opnieuw.'
      }
    }
    pin = ''
  }

  // Any parent request the server refuses with 401 means the session ended.
  $effect(() => {
    const locked = () => (unlocked = false)
    addEventListener('wistjet:locked', locked)
    return () => removeEventListener('wistjet:locked', locked)
  })

  async function lock() {
    await api('parent/logout', {}).catch(() => {})
    location.hash = '#/'
  }

  // Lock after 10 minutes without activity; the server enforces the same limit.
  $effect(() => {
    if (!unlocked) return
    let timer = setTimeout(lock, IDLE_MS)
    let lastPing = 0
    const reset = () => {
      clearTimeout(timer)
      timer = setTimeout(lock, IDLE_MS)
      // The server's idle clock only sees requests, so typing a long list would otherwise
      // lock the session mid-sentence. Tell it the parent is still here, at most once a minute.
      if (Date.now() - lastPing > 60_000) {
        lastPing = Date.now()
        api('parent/session').catch(() => {})
      }
    }
    const events = ['pointerdown', 'keydown'] as const
    events.forEach((e) => addEventListener(e, reset))
    return () => {
      clearTimeout(timer)
      events.forEach((e) => removeEventListener(e, reset))
    }
  })
</script>

{#if unlocked === null}
  <p class="notice">Laden…</p>
{:else if !unlocked}
  <form class="card narrow" onsubmit={unlock}>
    <h1>Ouderzone</h1>
    <label>
      Pincode
      <input type="password" inputmode="numeric" autocomplete="current-password" bind:value={pin} required />
    </label>
    {#if message}<p class="error" role="alert">{message}</p>{/if}
    <div class="actions">
      <a class="button secondary" href="#/">Terug</a>
      <button type="submit">Ontgrendel</button>
    </div>
  </form>
{:else}
  <header class="bar">
    <h1>Ouderzone</h1>
    <button class="secondary" onclick={lock}>Vergrendel</button>
  </header>
  <nav class="tabs">
    {#each TABS as [key, label] (key)}
      <a href="#/ouder/{key}" aria-current={(section?.[1] ?? 'overzicht') === key ? 'page' : undefined}>{label}</a>
    {/each}
  </nav>
  {#key route}
    {#if section?.[1] === 'kinderen'}
      <Children />
    {:else if section?.[1] === 'vakken'}
      <Subjects />
    {:else if section?.[1] === 'decks' && section[2]}
      <Deck id={Number(section[2])} />
    {:else if section?.[1] === 'decks'}
      <Decks />
    {:else if section?.[1] === 'systemen'}
      <Systems />
    {:else if section?.[1] === 'instellingen'}
      <Settings />
    {:else}
      <Overview />
    {/if}
  {/key}
{/if}
