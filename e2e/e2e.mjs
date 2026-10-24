// End-to-end test (spec §11): first run → create a deck by pasting a list → study a
// session → summary, through the real web app in headless Chrome. No dependencies: Node 22
// (built-in fetch and WebSocket) drives Chrome over the DevTools protocol.
//
//   node e2e/e2e.mjs target/release/wistjet
//
// Set CHROME to the browser binary if it isn't found. Exits 1 on the first failure.
// SCREENSHOTS=docs/images also saves the README screenshots on the way.

import { spawn } from 'node:child_process'
import { mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const bin = process.argv[2] ?? 'target/release/wistjet'
const chromeBin =
  process.env.CHROME ??
  (process.platform === 'darwin'
    ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
    : 'google-chrome')
const PORT = 18123
const DEVTOOLS = 9411
const BASE = `http://localhost:${PORT}`
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let server
let chrome
function stop(code) {
  server?.kill()
  chrome?.kill()
  process.exit(code)
}
function check(ok, what) {
  console.log(`${ok ? 'PASS' : 'FAIL'} ${what}`)
  if (!ok) stop(1)
}
setTimeout(() => {
  console.log('FAIL timed out')
  stop(1)
}, 120_000)

// ---- the app ----
server = spawn(bin, [], {
  env: {
    ...process.env,
    WISTJET_DATA_DIR: mkdtempSync(join(tmpdir(), 'wistjet-e2e-')),
    WISTJET_PORT: String(PORT),
    WISTJET_OPEN_BROWSER: '0',
  },
  stdio: ['ignore', 'pipe', 'inherit'],
})
let banner = ''
server.stdout.on('data', (d) => (banner += d))
for (let i = 0; i < 100 && !banner.includes('Sluit dit venster'); i++) await sleep(100)
const code = banner.match(/Instelcode:\s+(\d{6})/)?.[1]
check(Boolean(code), 'the app starts and prints a setup code')

// ---- the browser ----
chrome = spawn(
  chromeBin,
  [
    '--headless=new',
    '--disable-gpu',
    '--no-sandbox',
    `--remote-debugging-port=${DEVTOOLS}`,
    `--user-data-dir=${mkdtempSync(join(tmpdir(), 'wistjet-e2e-chrome-'))}`,
    '--window-size=1100,760',
    'about:blank',
  ],
  { stdio: 'ignore' },
)
let targets = []
// A cold CI runner can take well over 10 s to start Chrome.
for (let i = 0; i < 150 && !targets.some((t) => t.type === 'page'); i++) {
  await sleep(200)
  targets = await fetch(`http://127.0.0.1:${DEVTOOLS}/json`).then((r) => r.json(), () => [])
}
const page = targets.find((t) => t.type === 'page')
if (!page) throw new Error('Chrome did not open a page within 30 s')
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((r) => ws.addEventListener('open', r))
let nextId = 0
const pending = new Map()
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data)
  if (m.id && pending.has(m.id)) pending.get(m.id)(m)
  if (m.method === 'Page.javascriptDialogOpening') cdp('Page.handleJavaScriptDialog', { accept: true })
})
function cdp(method, params = {}) {
  return new Promise((resolve) => {
    const id = ++nextId
    pending.set(id, resolve)
    ws.send(JSON.stringify({ id, method, params }))
  })
}
const js = async (expression) =>
  (await cdp('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result.result?.value
const text = async () => (await js('document.body.innerText')) ?? ''
const go = async (url) => {
  await cdp('Page.navigate', { url })
  await sleep(1000)
}
/** Types into the nth element matching `selector`. */
async function type(selector, value, nth = 0) {
  await js(`document.querySelectorAll(${JSON.stringify(selector)})[${nth}].focus()`)
  await cdp('Input.insertText', { text: value })
}
/** Clicks the button or link whose text is exactly `label`. */
async function click(label) {
  const found = await js(`(() => {
    const el = [...document.querySelectorAll('button, a')].find((b) => b.textContent.trim() === ${JSON.stringify(label)})
    el?.click()
    return Boolean(el)
  })()`)
  check(found, `found "${label}"`)
  await sleep(500)
}
async function key(k) {
  const codes = { ' ': ['Space', 32], ArrowLeft: ['ArrowLeft', 37], ArrowRight: ['ArrowRight', 39] }
  for (const t of ['keyDown', 'keyUp'])
    await cdp('Input.dispatchKeyEvent', { type: t, key: k, code: codes[k][0], windowsVirtualKeyCode: codes[k][1] })
  await sleep(300)
}
await cdp('Page.enable')
async function shot(name) {
  if (!process.env.SCREENSHOTS) return
  const r = await cdp('Page.captureScreenshot', { format: 'png' })
  writeFileSync(join(process.env.SCREENSHOTS, `${name}.png`), Buffer.from(r.result.data, 'base64'))
}

// ---- first run: the browser opened by the app gets the code in its address ----
await go(`${BASE}/#setup=${code}`)
check((await text()).includes('Welkom bij Wistjet'), 'first run shows the setup wizard')
await type('input', 'De Smet')
await type('input[type=password]', '2468', 0)
await type('input[type=password]', '2468', 1)
await click('Volgende')
await type('input', 'Lotte')
await click('Volgende')
await js(`document.querySelectorAll('input[name=sample]')[1].click()`) // start empty
await click('Klaar')
check((await text()).includes('Klaar!'), 'setup finishes')
await click('Start')
check((await text()).includes('Wie gaat er oefenen?') && (await text()).includes('Lotte'), 'the profile screen shows the child')

// ---- the parent creates a subject and a deck by pasting a list ----
await click('🔒')
await type('input[type=password]', '2468')
await click('Ontgrendel')
check((await text()).includes('Overzicht'), 'the PIN opens the parent area')
await go(`${BASE}/#/ouder/vakken`)
await type('section.card input', 'Frans')
await click('Toevoegen')
await go(`${BASE}/#/ouder/decks`)
await type('section.card input', 'Woordjes H1')
await click('Toevoegen')
check((await text()).includes('Kaartjes (0)'), 'a new deck opens on its own page')
await type('textarea', 'le chat - de kat\nle chien - de hond\nla maison - het huis')
await click('Bekijk')
await click('Voeg 3 kaartjes toe')
check((await text()).includes('Kaartjes (3)'), 'pasting a list creates 3 cards')
await shot('parent-deck')
await click('Vergrendel')

// ---- the child studies ----
await go(`${BASE}/#/`)
await js(`document.querySelector('a.tile').click()`)
await sleep(800)
check((await text()).includes('3 kaartjes om te oefenen'), 'the child sees 3 cards due')
check(/Zo ver ben je[\s\S]*vandaag: elke dag/.test(await text()), 'the home screen shows the boxes and today\'s box')
await shot('child-home')
// Only the second card asks to type its answer.
await js(`(() => { const rolls = [0.5, 0.1]; Math.random = () => rolls.shift() ?? 0.5 })()`)
await click('Start met oefenen')
check((await text()).includes('1 van 3'), 'the session starts at 1 van 3')
await key(' ')
await shot('study')
await key('ArrowLeft') // not known: comes back at the end
check((await text()).includes('2 van 4'), 'a missed card comes back at the end')
await key(' ')
check((await text()).includes('Typ je antwoord'), 'a check card asks to type the answer, space does not skip it')
const pairs = { 'le chat': 'de kat', 'le chien': 'de hond', 'la maison': 'het huis' }
const question = await js(`document.querySelector('.card-text').textContent`)
const right = pairs[question] ?? Object.keys(pairs).find((k) => pairs[k] === question)
await type('.type-answer input', right.toUpperCase())
await click('Controleer')
check((await text()).includes('Goed getypt!'), 'the typed answer is checked')
await click('Verder')
for (let i = 0; i < 2; i++) {
  await key(' ')
  await key('ArrowRight')
}
await sleep(500) // the summary fetches where the cards stand now
const summary = (await text()).replace(/\s+/g, ' ')
check(summary.includes('Goed gedaan!'), 'the summary appears')
await shot('summary')
check(/2 wist je 1 nog niet 1 keer herhaald/.test(summary), `the summary counts known, not known and repeats (${summary})`)
check(summary.includes('om de 2 weken →'), 'the summary shows the box bar')
check(summary.includes('2 kaartjes een bakje verder'), 'the summary counts the cards that moved up')

console.log('All end-to-end checks passed.')
stop(0)
