<script lang="ts">
  import { COLOURS } from '../lib/colours'

  let { value = $bindable(), name }: { value: string; name: string } = $props()
</script>

<fieldset class="swatches">
  <legend>Kleur</legend>
  {#each COLOURS as colour, i (colour)}
    <label class="swatch" style:background={colour}>
      <input type="radio" {name} value={colour} bind:group={value} />
      <span class="sr-only">Kleur {i + 1}</span>
    </label>
  {/each}
</fieldset>
