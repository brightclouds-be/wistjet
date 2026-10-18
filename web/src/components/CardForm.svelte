<script lang="ts">
  import { upload } from '../lib/api'
  import { problem } from '../lib/messages'
  import type { CardInput } from '../lib/types'

  let {
    card = $bindable(),
    submitLabel,
    onsubmit,
    oncancel,
  }: { card: CardInput; submitLabel: string; onsubmit: () => void; oncancel?: () => void } = $props()

  const MAX_BYTES = 5 * 1024 * 1024
  let uploadError = $state('')
  let uploading = $state(false)

  async function pick(side: 'front' | 'back', e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    uploadError = ''
    if (file.size > MAX_BYTES) {
      uploadError = 'Een afbeelding is hoogstens 5 MB.'
      return
    }
    uploading = true
    try {
      const { id } = await upload<{ id: number }>('parent/images', file)
      if (side === 'front') card.front_image = id
      else card.back_image = id
    } catch (err) {
      uploadError = problem(err)
    } finally {
      uploading = false
    }
  }

  function submit(e: SubmitEvent) {
    e.preventDefault()
    onsubmit()
  }
</script>

{#snippet image(id: number | null, side: 'front' | 'back', remove: () => void)}
  {#if id !== null}
    <img class="thumb" src="/api/images/{id}" alt="" />
    <button type="button" class="secondary small" onclick={remove}>Afbeelding weg</button>
  {:else}
    <label class="file">
      + Afbeelding
      <input type="file" accept="image/jpeg,image/png,image/webp,image/gif" onchange={(e) => pick(side, e)} />
    </label>
  {/if}
{/snippet}

<form class="card-form" onsubmit={submit}>
  <div class="sides">
    <fieldset class="side">
      <legend>Voorkant</legend>
      <input bind:value={card.front_text} aria-label="Tekst voorkant" />
      {@render image(card.front_image, 'front', () => (card.front_image = null))}
    </fieldset>
    <fieldset class="side">
      <legend>Achterkant</legend>
      <input bind:value={card.back_text} aria-label="Tekst achterkant" />
      {@render image(card.back_image, 'back', () => (card.back_image = null))}
    </fieldset>
  </div>
  <label>Notities (optioneel) <input bind:value={card.notes} /></label>
  {#if uploadError}<p class="error" role="alert">{uploadError}</p>{/if}
  <div class="actions">
    {#if oncancel}<button type="button" class="secondary" onclick={oncancel}>Annuleer</button>{/if}
    <button type="submit" disabled={uploading}>{submitLabel}</button>
  </div>
</form>
