<script lang="ts">
  import { api, ApiError } from '../lib/api'

  let {
    lanUrl,
    codeRequired,
    onDone,
  }: { lanUrl: string; codeRequired: boolean; onDone: () => void } = $props()

  const COLOURS = ['#e57373', '#f06292', '#ba68c8', '#7986cb', '#4fc3f7', '#4db6ac', '#aed581', '#ffb74d']
  const FIELD_STEP: Record<string, number> = { family_name: 1, timezone: 1, pin: 1, child_name: 2, child_avatar: 2 }
  const FIELD_MESSAGE: Record<string, string> = {
    family_name: 'Geef een familienaam van hoogstens 50 tekens.',
    timezone: 'Kies een tijdzone uit de lijst.',
    pin: 'De pincode bestaat uit 4 tot 8 cijfers.',
    child_name: 'Geef een naam van hoogstens 50 tekens.',
    child_avatar: 'Kies een kleur.',
  }

  const browserZone = Intl.DateTimeFormat().resolvedOptions().timeZone
  const zones = Intl.supportedValuesOf('timeZone')
  if (!zones.includes(browserZone)) zones.unshift(browserZone)

  let step = $state(1)
  let familyName = $state('')
  let timezone = $state(browserZone)
  let pin = $state('')
  let pinAgain = $state('')
  let setupCode = $state('')
  let childName = $state('')
  let childAvatar = $state(COLOURS[0])
  let sampleDeck = $state(true)
  let error = $state('')
  let busy = $state(false)

  function next(e: SubmitEvent) {
    e.preventDefault()
    error = ''
    if (step === 1 && !/^\d{4,8}$/.test(pin)) {
      error = FIELD_MESSAGE.pin
      return
    }
    if (step === 1 && pin !== pinAgain) {
      error = 'De twee pincodes zijn niet gelijk.'
      return
    }
    step += 1
  }

  function back() {
    error = ''
    step -= 1
  }

  async function finish(e: SubmitEvent) {
    e.preventDefault()
    error = ''
    busy = true
    try {
      await api('setup', {
        family_name: familyName,
        timezone,
        pin,
        child_name: childName,
        child_avatar: childAvatar,
        sample_deck: sampleDeck,
        setup_code: setupCode,
      })
      step = 4
    } catch (err) {
      const field = err instanceof ApiError ? err.body?.field : undefined
      if (err instanceof ApiError && err.status === 409) {
        onDone() // set up meanwhile, e.g. in another tab
      } else if (err instanceof ApiError && err.body?.error === 'setup_code') {
        step = 1
        error = 'Die instelcode klopt niet. Je vindt hem in het Wistjet-venster op de computer.'
      } else if (err instanceof ApiError && err.body?.error === 'setup_code_locked') {
        step = 1
        error = 'Te veel foute instelcodes. Rond de installatie af op de computer waar Wistjet draait, of herstart Wistjet.'
      } else if (field && field in FIELD_STEP) {
        step = FIELD_STEP[field]
        error = FIELD_MESSAGE[field]
      } else {
        error = 'Opslaan lukte niet. Probeer het opnieuw.'
      }
    } finally {
      busy = false
    }
  }
</script>

<div class="card narrow">
  {#if step < 4}<p class="steps">Stap {step} van 3</p>{/if}

  {#if step === 1}
    <form onsubmit={next}>
      <h1>Welkom bij Wistjet</h1>
      {#if codeRequired}
        <label>
          Instelcode (staat in het Wistjet-venster op de computer)
          <input inputmode="numeric" autocomplete="one-time-code" bind:value={setupCode} required />
        </label>
      {/if}
      <label>Familienaam <input bind:value={familyName} maxlength="50" required /></label>
      <label>
        Tijdzone
        <select bind:value={timezone}>
          {#each zones as zone (zone)}<option>{zone}</option>{/each}
        </select>
      </label>
      <label>
        Pincode voor de ouderzone (4 tot 8 cijfers)
        <input type="password" inputmode="numeric" autocomplete="new-password" bind:value={pin} required />
      </label>
      <label>
        Herhaal de pincode
        <input type="password" inputmode="numeric" autocomplete="new-password" bind:value={pinAgain} required />
      </label>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions"><button type="submit">Volgende</button></div>
    </form>
  {:else if step === 2}
    <form onsubmit={next}>
      <h1>Wie gaat er oefenen?</h1>
      <label>Naam van je kind <input bind:value={childName} maxlength="50" required /></label>
      <fieldset class="swatches">
        <legend>Kleur</legend>
        {#each COLOURS as colour, i (colour)}
          <label class="swatch" style:background={colour}>
            <input type="radio" name="colour" value={colour} bind:group={childAvatar} />
            <span class="sr-only">Kleur {i + 1}</span>
          </label>
        {/each}
      </fieldset>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button type="button" class="secondary" onclick={back}>Terug</button>
        <button type="submit">Volgende</button>
      </div>
    </form>
  {:else if step === 3}
    <form onsubmit={finish}>
      <h1>Meteen proberen?</h1>
      <label class="choice">
        <input type="radio" name="sample" value={true} bind:group={sampleDeck} />
        Ja, voeg een klein voorbeelddeck Frans – Nederlands toe
      </label>
      <label class="choice">
        <input type="radio" name="sample" value={false} bind:group={sampleDeck} />
        Nee, ik begin met een lege Wistjet
      </label>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button type="button" class="secondary" onclick={back}>Terug</button>
        <button type="submit" disabled={busy}>Klaar</button>
      </div>
    </form>
  {:else}
    <h1>Klaar!</h1>
    <p>Open Wistjet op een tablet in hetzelfde wifi-netwerk door deze code te scannen:</p>
    <img class="qr" src="/api/lan-qr.svg" alt="QR-code naar {lanUrl}" />
    <p class="url">{lanUrl}</p>
    <div class="actions"><button onclick={onDone}>Start</button></div>
  {/if}
</div>
