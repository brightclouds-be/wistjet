<script lang="ts">
  import { addBox, moveBox, removeBox, type Editor } from '../lib/systemEditor'

  let {
    editor = $bindable(),
    busy,
    onsave,
    oncancel,
  }: { editor: Editor; busy: boolean; onsave: () => void; oncancel: () => void } = $props()

  function submit(e: SubmitEvent) {
    e.preventDefault()
    onsave()
  }
</script>

<form onsubmit={submit}>
  <label>Naam <input bind:value={editor.name} required /></label>
  <label class="choice">
    <input type="checkbox" bind:checked={editor.repeat_missed} />
    Gemiste kaartjes komen terug in dezelfde oefenbeurt
  </label>

  <div class="scroll">
    <table class="boxes">
      <thead>
        <tr><th>Bakje</th><th>Elke … dagen</th><th>Wist ik →</th><th>Wist ik niet →</th><th></th></tr>
      </thead>
      <tbody>
        {#each editor.boxes as box, i (box.key)}
          <tr>
            <td><input bind:value={box.name} aria-label="Naam van bakje {i + 1}" required /></td>
            <td>
              <input type="number" min="1" max="365" bind:value={box.interval_days} aria-label="Dagen voor {box.name}" required />
            </td>
            <td>
              <select bind:value={box.success} aria-label="{box.name}: na Wist ik">
                {#each editor.boxes as target (target.key)}<option value={target.key}>{target.name}</option>{/each}
              </select>
            </td>
            <td>
              <select bind:value={box.failure} aria-label="{box.name}: na Wist ik niet">
                {#each editor.boxes as target (target.key)}<option value={target.key}>{target.name}</option>{/each}
              </select>
            </td>
            <td class="box-actions">
              <button type="button" class="secondary small" disabled={i === 0} aria-label="{box.name} hoger"
                onclick={() => (editor = moveBox(editor, box.key, -1))}>↑</button>
              <button type="button" class="secondary small" disabled={i === editor.boxes.length - 1} aria-label="{box.name} lager"
                onclick={() => (editor = moveBox(editor, box.key, 1))}>↓</button>
              <button type="button" class="secondary small" disabled={editor.boxes.length === 1} aria-label="{box.name} weghalen"
                onclick={() => (editor = removeBox(editor, box.key))}>✕</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <button type="button" class="secondary" disabled={editor.boxes.length >= 12} onclick={() => (editor = addBox(editor))}>
    + Bakje
  </button>

  {#if editor.notices.length > 0}
    <div class="notices" role="status">
      <p>Door het weghalen van een bakje is dit aangepast:</p>
      <ul>{#each editor.notices as notice, i (i)}<li>{notice}</li>{/each}</ul>
    </div>
  {/if}

  <div class="actions">
    <button type="button" class="secondary" onclick={oncancel}>Annuleer</button>
    <button type="submit" disabled={busy}>Bewaar</button>
  </div>
</form>
