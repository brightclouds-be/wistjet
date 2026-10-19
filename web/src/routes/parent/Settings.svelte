<script lang="ts">
  import { api } from '../../lib/api'
  import { problem } from '../../lib/messages'
  import type { Settings, SystemDetail } from '../../lib/types'

  let settings = $state<Settings | null>(null)
  let systems = $state<SystemDetail[]>([])
  let zones = $state<string[]>([])
  let pins = $state({ current: '', new: '', again: '' })
  let status = $state({ settings: '', pin: '' })
  let errors = $state({ settings: '', pin: '' })
  let busy = $state(false)

  async function load() {
    ;[settings, systems] = await Promise.all([api<Settings>('parent/settings'), api<SystemDetail[]>('parent/systems')])
    zones = Intl.supportedValuesOf('timeZone')
    if (!zones.includes(settings.timezone)) zones = [settings.timezone, ...zones]
  }
  load().catch(() => (errors.settings = 'De instellingen konden niet geladen worden.'))

  async function run(section: 'settings' | 'pin', action: () => Promise<unknown>, done: string) {
    if (busy) return
    busy = true
    errors[section] = ''
    status[section] = ''
    try {
      await action()
      status[section] = done
    } catch (err) {
      errors[section] = problem(err)
    } finally {
      busy = false
    }
  }

  function save(e: SubmitEvent) {
    e.preventDefault()
    if (settings) run('settings', () => api('parent/settings', settings, 'PUT'), 'Bewaard.')
  }

  function changePin(e: SubmitEvent) {
    e.preventDefault()
    if (pins.new !== pins.again) {
      errors.pin = 'De twee nieuwe pincodes zijn niet gelijk.'
      return
    }
    run(
      'pin',
      async () => {
        await api('parent/pin', { current: pins.current, new: pins.new })
        pins = { current: '', new: '', again: '' }
      },
      'De pincode is gewijzigd. Andere toestellen moeten opnieuw ontgrendelen.',
    )
  }
</script>

{#if settings}
  <section class="card">
    <h2>Gezin</h2>
    <form onsubmit={save}>
      <label>Familienaam <input bind:value={settings.name} required /></label>
      <label>
        Tijdzone
        <select bind:value={settings.timezone}>
          {#each zones as zone (zone)}<option>{zone}</option>{/each}
        </select>
      </label>
      <label>
        Standaard studiesysteem voor nieuwe decks
        <select bind:value={settings.default_system_id}>
          {#each systems as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
        </select>
      </label>
      {#if errors.settings}<p class="error" role="alert">{errors.settings}</p>{/if}
      {#if status.settings}<p class="saved" role="status">{status.settings}</p>{/if}
      <div class="actions"><button type="submit" disabled={busy}>Bewaar</button></div>
    </form>
  </section>
{:else if errors.settings}
  <p class="error" role="alert">{errors.settings}</p>
{/if}

<section class="card">
  <h2>Pincode wijzigen</h2>
  <form onsubmit={changePin}>
    <label>
      Huidige pincode
      <input type="password" inputmode="numeric" autocomplete="current-password" bind:value={pins.current} required />
    </label>
    <label>
      Nieuwe pincode (4 tot 8 cijfers)
      <input type="password" inputmode="numeric" autocomplete="new-password" bind:value={pins.new} required />
    </label>
    <label>
      Herhaal de nieuwe pincode
      <input type="password" inputmode="numeric" autocomplete="new-password" bind:value={pins.again} required />
    </label>
    {#if errors.pin}<p class="error" role="alert">{errors.pin}</p>{/if}
    {#if status.pin}<p class="saved" role="status">{status.pin}</p>{/if}
    <div class="actions"><button type="submit" disabled={busy}>Wijzig pincode</button></div>
  </form>
</section>
