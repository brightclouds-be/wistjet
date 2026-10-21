<script lang="ts">
  import { api } from '../lib/api'
  import { initial, whenNext } from '../lib/text'
  import BoxBars, { type SubjectBoxes } from '../components/BoxBars.svelte'

  type Home = {
    name: string
    avatar: string
    due: number
    subjects: { id: number; name: string; due: number }[]
    answered_today: number
    next_due: string | null
    progress: SubjectBoxes[]
  }

  let { childId }: { childId: number } = $props()

  const home = $derived(api<Home>(`children/${childId}/home`))
</script>

{#await home}
  <p class="notice">Laden…</p>
{:then h}
  <header class="bar">
    <h1 class="greeting">
      <span class="avatar small" style:background={h.avatar}>{initial(h.name)}</span>
      Hoi {h.name}!
    </h1>
    <a class="button secondary" href="#/">Iemand anders</a>
  </header>

  {#if h.due === 0}
    <div class="card cheer">
      <h2>Klaar voor vandaag! 🎉</h2>
      {#if h.next_due}<p>De volgende kaartjes komen {whenNext(h.next_due)}.</p>{/if}
    </div>
  {:else}
    {@const total = h.answered_today + h.due}
    <div class="card today">
      <p class="due"><strong>{h.due}</strong> {h.due === 1 ? 'kaartje' : 'kaartjes'} om te oefenen</p>
      <div
        class="progress"
        role="progressbar"
        aria-label="Vandaag al geoefend"
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={h.answered_today}
      >
        <span style:width="{(100 * h.answered_today) / total}%"></span>
      </div>
      <a class="button big" href="#/kind/{childId}/oefenen">Start met oefenen</a>
    </div>

    <h2>Of kies een vak</h2>
    <ul class="subjects">
      {#each h.subjects as subject (subject.id)}
        <li>
          <a class="subject" href="#/kind/{childId}/oefenen/{subject.id}">
            <span>{subject.name}</span>
            <span class="badge">{subject.due}</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}

  {#if h.progress.length}
    <h2>Zo ver ben je</h2>
    <BoxBars subjects={h.progress} />
  {/if}
{:catch}
  <div class="card narrow">
    <p>Dit profiel bestaat niet (meer).</p>
    <div class="actions"><a class="button" href="#/">Terug</a></div>
  </div>
{/await}
