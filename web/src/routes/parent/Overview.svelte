<script lang="ts">
  import { api } from '../../lib/api'
  import { initial } from '../../lib/text'
  import type { ChildOverview } from '../../lib/types'

  const overview = api<ChildOverview[]>('parent/overview')
</script>

{#await overview}
  <p class="notice">Laden…</p>
{:then children}
  <div class="overview">
    {#each children as c (c.child_id)}
      {@const most = Math.max(1, ...c.boxes)}
      <section class="card">
        <h2 class="greeting">
          <span class="avatar small" style:background={c.avatar}>{initial(c.name)}</span>
          {c.name}
        </h2>
        <dl class="stats">
          <div><dt>Nog te oefenen</dt><dd>{c.due}</dd></div>
          <div><dt>Vandaag geoefend</dt><dd>{c.reviewed}</dd></div>
          <div><dt>Wist het</dt><dd>{c.known}</dd></div>
          <div><dt>Nog niet</dt><dd>{c.not_known}</dd></div>
        </dl>
        {#if c.rushed >= 5 && c.rushed * 2 >= c.known}
          <p class="rushed">
            ⚠ Vandaag {c.rushed} van de {c.known} keer "Wist ik" binnen 2 seconden. Klikt {c.name} snel door?
          </p>
        {/if}
        {#if c.boxes.length > 0}
          <h3>Kaartjes per bakje</h3>
          <ul class="box-counts">
            {#each c.boxes as count, i (i)}
              <li>
                <span class="label">Bakje {i + 1}</span>
                <span class="track"><span class="bar" style:width="{(100 * count) / most}%"></span></span>
                <span class="count">{count}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted">Nog geen kaartjes.</p>
        {/if}
      </section>
    {:else}
      <p class="muted">Nog geen kinderen.</p>
    {/each}
  </div>
{:catch}
  <p class="error" role="alert">Het overzicht kon niet geladen worden.</p>
{/await}
