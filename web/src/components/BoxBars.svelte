<script module lang="ts">
  /** A subject's cards per box, first box first (see `src/study.rs`). */
  export type SubjectBoxes = {
    id: number
    name: string
    boxes: { interval_days: number; cards: number; due: number }[]
  }
</script>

<script lang="ts">
  import { howOften } from '../lib/text'

  let { subjects }: { subjects: SubjectBoxes[] } = $props()

  /** From orange (first box) to green (last box), whatever the number of boxes. */
  const colour = (i: number, n: number) =>
    `color-mix(in oklch, var(--yes) ${n === 1 ? 100 : (100 * i) / (n - 1)}%, var(--accent))`
</script>

<ul class="box-bars">
  {#each subjects as s (s.id)}
    {@const total = s.boxes.reduce((sum, b) => sum + b.cards, 0)}
    {@const today = s.boxes.filter((b) => b.due > 0).map((b) => howOften(b.interval_days))}
    <li>
      <h3>
        {s.name}
        {#if today.length}<span class="today-boxes">· vandaag: {today.join(', ')}</span>{/if}
      </h3>
      <div
        class="box-bar"
        role="img"
        aria-label="{s.name}: {s.boxes.map((b) => `${howOften(b.interval_days)} ${b.cards}`).join(', ')}"
      >
        {#each s.boxes as b, i (i)}
          {#if b.cards > 0}
            <span style:width="{(100 * b.cards) / total}%" style:background={colour(i, s.boxes.length)}>
              {#if b.cards / total >= 0.12}{b.cards}{/if}
            </span>
          {/if}
        {/each}
      </div>
      <div class="box-legend" aria-hidden="true">
        <span>{howOften(s.boxes[0].interval_days)}</span>
        <span>{howOften(s.boxes[s.boxes.length - 1].interval_days)} →</span>
      </div>
    </li>
  {/each}
</ul>
