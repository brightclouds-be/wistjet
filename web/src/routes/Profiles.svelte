<script lang="ts">
  import { api } from '../lib/api'
  import { initial } from '../lib/text'

  type Child = { id: number; name: string; avatar: string; due: number }

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
      <li>
        <a class="tile" href="#/kind/{child.id}">
          <span class="avatar" style:background={child.avatar}>{initial(child.name)}</span>
          <span class="tile-name">{child.name}</span>
          {#if child.due > 0}
            <span class="badge" aria-label="{child.due} kaartjes te oefenen">{child.due}</span>
          {:else}
            <span class="badge done" aria-label="Klaar voor vandaag">✓</span>
          {/if}
        </a>
      </li>
    {/each}
  </ul>
{:catch}
  <p class="notice">De kinderen konden niet geladen worden.</p>
{/await}
