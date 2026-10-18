<script lang="ts">
  import { api } from '../lib/api'
  import { initial } from '../lib/text'

  type Child = { id: number; name: string; avatar: string }

  const children = api<Child[]>('children')
</script>

<header class="bar">
  <h1>Wie gaat er oefenen?</h1>
  <a class="icon-button" href="#/ouder" aria-label="Ouderzone" title="Ouderzone">🔒</a>
</header>

{#await children}
  <p class="notice">Laden…</p>
{:then list}
  <ul class="tiles">
    {#each list as child (child.id)}
      <li class="tile">
        <span class="avatar" style:background={child.avatar}>{initial(child.name)}</span>
        <span class="tile-name">{child.name}</span>
      </li>
    {/each}
  </ul>
{:catch}
  <p class="notice">De kinderen konden niet geladen worden.</p>
{/await}
