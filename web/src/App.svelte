<script lang="ts">
  import { api } from './lib/api'
  import ChildHome from './routes/ChildHome.svelte'
  import Parent from './routes/Parent.svelte'
  import Profiles from './routes/Profiles.svelte'
  import Setup from './routes/Setup.svelte'
  import Study from './routes/Study.svelte'

  type Status = { setup_done: boolean; lan_url: string }

  let status = $state<Status | null>(null)
  let failed = $state(false)
  let route = $state(location.hash)
  // #/kind/<child>, #/kind/<child>/oefenen, #/kind/<child>/oefenen/<subject>
  const child = $derived(route.match(/^#\/kind\/(\d+)(\/oefenen(?:\/(\d+))?)?$/))

  $effect(() => {
    const onHash = () => (route = location.hash)
    addEventListener('hashchange', onHash)
    return () => removeEventListener('hashchange', onHash)
  })

  async function load() {
    try {
      status = await api<Status>('status')
      failed = false
    } catch {
      failed = true
    }
  }
  load()
</script>

<main>
  {#if failed}
    <div class="card narrow">
      <p>Wistjet is niet bereikbaar. Draait het programma nog?</p>
      <div class="actions"><button onclick={load}>Opnieuw proberen</button></div>
    </div>
  {:else if !status}
    <p class="notice">Laden…</p>
  {:else if !status.setup_done}
    <Setup lanUrl={status.lan_url} onDone={load} />
  {:else if route.startsWith('#/ouder')}
    <Parent {route} />
  {:else if child?.[2]}
    {#key route}<Study childId={Number(child[1])} subject={child[3] ? Number(child[3]) : null} />{/key}
  {:else if child}
    {#key route}<ChildHome childId={Number(child[1])} />{/key}
  {:else}
    <Profiles />
  {/if}
</main>
