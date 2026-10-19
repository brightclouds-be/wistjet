# Plan 5b: Distribution — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Other families can install Wistjet with one download, one `docker run`/`docker compose up`, or one `kubectl apply`, read how in the README, and get new versions from a tagged release (spec §9.1).

**Architecture:** No new app features beyond one setting.
- **`WISTJET_PUBLIC_URL`** is the tablet address shown during setup, because in a container the detected address is the container's own.
- **Docker image:** a multi-stage `Dockerfile`. Node builds the web app, Rust builds the binary that embeds it, and the result runs on `distroless/cc` as a non-root user with `/data` as the volume.
- **Compose and Kubernetes examples** for NAS and home clusters.
- **A release workflow** for tags: binaries for five platforms, plus the multi-arch image on `ghcr.io`.
- **Docs:** an English README, the AGPL-3.0 license, and screenshots that the end-to-end test can regenerate.

**Tech Stack:** Docker (BuildKit), `gcr.io/distroless/cc-debian12:nonroot`, GitHub Actions (`softprops/action-gh-release`, `docker/*` actions). No new Rust or npm dependencies.

**Spec:** `docs/superpowers/specs/2026-10-08-family-flashcards-design.md` (§2, §8 "README must state…", §9.1). Roadmap: `docs/superpowers/plans/2026-10-11-wistjet-roadmap.md`.

## Decisions made while planning

The first three are the family's answers to questions asked while planning.
- **License: AGPL-3.0**, declared as `AGPL-3.0-only`. The `LICENSE` file is the unchanged text from <https://www.gnu.org/licenses/agpl-3.0.txt> (SHA-256 `0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0`).
- **No tray icon; the console window stays,** on every platform. It shows the addresses and the setup code, and closing it stops Wistjet. A tray needs a GUI event loop and extra Linux build libraries, and couldn't be tested on Windows or a Linux desktop.
- **README in English.**
- **Plain archives, not installers.** Releases are `tar.gz`/`zip` archives built by a plain GitHub Actions matrix rather than `cargo-dist`: a short workflow to read, with nothing generated. Installers and code signing can come later. The README walks through the Windows and macOS "unknown developer" warnings.
- **The image** runs as uid 65532. Its `/data` is created owned by that user, so a new named volume is writable. Kubernetes uses `fsGroup: 65532`. A NAS folder for backups must be writable by that user, as the comments say.
- **The Kubernetes example** uses `replicas: 1` and `strategy: Recreate`, because one SQLite file must never be shared by two pods. The probes use `/api/status`: the kubelet calls the pod by IP, which the host check allows.
- **arm64 images are built under QEMU** in the release workflow. Simple but slow; marked with a `ponytail:` note.
- **Screenshots** come from the end-to-end test (`SCREENSHOTS=docs/images`), so the README can always be refreshed from the real app.

## Global Constraints

- The README must state that Wistjet is not meant to be exposed to the internet (spec §8).
- One command or one short config file installs it, on a single machine and on a home cluster (spec §9.1).
- Data persists across restarts and upgrades, in one configurable place (spec §9.1).
- All checks from earlier plans stay green, including `node e2e/e2e.mjs`.

## Review Focus

1. **A new Docker volume:** the non-root app can write to it, and the database and daily backups land there. Task 2 run-through.
2. **A container restart or upgrade:** the family's data is still there. Task 2 run-through.
3. **Tablets in a Docker or Kubernetes setup:** the setup QR code points at the server, not at a container address. `WISTJET_PUBLIC_URL`, test `overrides` (Task 1), Task 2 run-through.
4. **Two pods on one volume during a Kubernetes rollout:** prevented by `Recreate`. Task 3 manifest.
5. **A first-time Windows or macOS user** facing the "unknown developer" warning: the README's steps get them through it. Task 5.

## File Structure

| File | Responsibility |
|---|---|
| `src/main.rs` | + `WISTJET_PUBLIC_URL` |
| `Dockerfile`, `.dockerignore` | The image |
| `compose.yaml` | Docker Compose example |
| `k8s/wistjet.yaml` | Kubernetes example: PVC, Deployment, Service, Ingress |
| `.github/workflows/release.yml` | Binaries and image on a version tag |
| `README.md`, `LICENSE`, `docs/images/*.png` | Documentation |
| `Cargo.toml`, `web/package.json` | `license` field |
| `e2e/e2e.mjs` | Optional screenshots |

---

### Task 1: Tablet address setting

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Produces: environment variable `WISTJET_PUBLIC_URL`, used as the address in the banner, `/api/status` and the setup QR code.

- [ ] **Step 1: Write the failing test**

In the `tests` module of `src/main.rs`:
- in `defaults`, add `public_url: None` to the expected `Config`;
- in `overrides`, add `("WISTJET_PUBLIC_URL", "http://nas.local:8080"),` to the variables and `public_url: Some("http://nas.local:8080".into())` to the expected `Config`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --bin wistjet`
Expected: compile error, `struct Config has no field named public_url`.

- [ ] **Step 3: Implement**

Add the field at the end of `struct Config`:
```rust
    /// The address tablets use, when the detected one is wrong (in a container it is the
    /// container's own address). Shown in the banner and as the setup QR code.
    public_url: Option<String>,
```
add `public_url: get("WISTJET_PUBLIC_URL"),` at the end of the `Config { … }` literal in `from_env`, and replace the `let lan_url = …;` statement in `run` with:
```rust
    let lan_url = config.public_url.clone().unwrap_or_else(|| {
        local_ip_address::local_ip()
            .map(|ip| format!("http://{ip}:{}", config.port))
            .unwrap_or_else(|_| local.clone())
    });
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --bin wistjet && cargo clippy --all-targets -- -D warnings`
Expected: 7 tests pass; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: WISTJET_PUBLIC_URL for the tablet address in containers"
```

---

### Task 2: Docker image and Compose

**Files:**
- Create: `Dockerfile`, `.dockerignore`, `compose.yaml`

- [ ] **Step 1: Write the image**

`Dockerfile`:
```dockerfile
# Wistjet in one small image: the web app is built, then embedded in the Rust binary, which
# runs as a non-root user with all data in the /data volume.
#
#   docker build -t wistjet .
#   docker run -d -p 8080:8080 -v wistjet-data:/data wistjet

FROM node:22-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:1-slim-bookworm AS app
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY --from=web /src/web/dist ./web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/wistjet /wistjet
# An empty folder for the volume, owned by the user the app runs as, so a new volume is writable.
RUN mkdir /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=app /wistjet /usr/local/bin/wistjet
COPY --from=app --chown=nonroot:nonroot /data /data
ENV WISTJET_DATA_DIR=/data \
    WISTJET_PORT=8080 \
    WISTJET_OPEN_BROWSER=0
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/wistjet"]
```

`.dockerignore`:
```
target
web/node_modules
web/dist
.git
.superpowers
```

`compose.yaml` (an `environment:` block needs at least one real entry, so the backup count is set explicitly):
```yaml
# Wistjet with Docker Compose:
#
#   docker compose up -d
#   docker compose logs wistjet     # shows the setup code for the first run
#
# Then open http://<this machine>:8080 and finish setup. All data lives in the
# `wistjet-data` volume; back it up, or let Wistjet back up daily to a folder of your choice.
services:
  wistjet:
    image: ghcr.io/brightclouds-be/wistjet:latest
    # Or build it yourself from this folder:
    # build: .
    restart: unless-stopped
    ports:
      - "8080:8080"
    environment:
      # The address tablets use, shown during setup. Fill in this machine's address.
      # WISTJET_PUBLIC_URL: "http://192.168.1.10:8080"
      # Extra host names that may reach Wistjet (e.g. behind a reverse proxy).
      # WISTJET_HOSTS: "wistjet.thuis"
      # Daily backups to a NAS folder (mounted below) instead of inside the data volume.
      # WISTJET_BACKUP_DIR: "/backups"
      # How many daily backups to keep.
      WISTJET_BACKUP_KEEP: "14"
    volumes:
      - wistjet-data:/data
      # The folder must be writable by user id 65532, the user Wistjet runs as.
      # - /mnt/nas/wistjet-backups:/backups

volumes:
  wistjet-data:
```

- [ ] **Step 2: Build and run it**

Run:
```bash
docker build -t wistjet:test .
docker run -d --name wj-test -p 18200:8080 -e WISTJET_PUBLIC_URL=http://nas.local:18200 -v wj-test-data:/data wistjet:test
sleep 2; docker logs wj-test
```
Expected: the image is about 60 MB; the log shows `Op een tablet: http://nas.local:18200`, `Gegevens: /data`, `Back-ups: /data/backups` and an `Instelcode`.

- [ ] **Step 3: Set up, restart, and check the volume**

Run:
```bash
CODE=$(docker logs wj-test 2>&1 | grep -oE 'Instelcode: +[0-9]{6}' | grep -oE '[0-9]{6}')
curl -s -o /dev/null -w "setup %{http_code}\n" -H 'Content-Type: application/json' \
  -d "{\"family_name\":\"De Smet\",\"timezone\":\"Europe/Brussels\",\"pin\":\"2468\",\"child_name\":\"Lotte\",\"child_avatar\":\"#e57373\",\"sample_deck\":true,\"setup_code\":\"$CODE\"}" \
  http://localhost:18200/api/setup
docker restart wj-test; sleep 5
curl -s http://localhost:18200/api/children; echo
docker run --rm -v wj-test-data:/data alpine ls -ln /data /data/backups
docker rm -f wj-test; docker volume rm wj-test-data
```
Expected: `setup 204`; after the restart, Lotte is still listed with 10 cards due; `/data` holds `wistjet.db` owned by uid 65532, and `/data/backups` holds `wistjet-<today>.db`.

- [ ] **Step 4: Compose**

Run:
```bash
docker tag wistjet:test ghcr.io/brightclouds-be/wistjet:latest
docker compose -p wjcompose up -d && sleep 3 && docker compose -p wjcompose logs wistjet
curl -s localhost:8080/api/status; echo
docker compose -p wjcompose down -v; docker rmi ghcr.io/brightclouds-be/wistjet:latest
```
Expected: the log shows `Wistjet draait.` and an `Instelcode`; status answers `setup_done: false`.

- [ ] **Step 5: Commit**

```bash
git add Dockerfile .dockerignore compose.yaml
git commit -m "feat: Docker image and Compose file"
```

---

### Task 3: Kubernetes example

**Files:**
- Create: `k8s/wistjet.yaml`

- [ ] **Step 1: Write the manifest**

`k8s/wistjet.yaml`:
```yaml
# Wistjet on a home Kubernetes cluster:
#
#   kubectl apply -f k8s/wistjet.yaml
#   kubectl logs deploy/wistjet        # shows the setup code for the first run
#
# Wistjet keeps its data in one SQLite file, so it runs as exactly one pod: `replicas: 1`
# and the `Recreate` strategy (never two pods writing to the same volume).
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: wistjet-data
spec:
  accessModes: [ReadWriteOnce]
  resources:
    requests:
      storage: 2Gi
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: wistjet
spec:
  replicas: 1
  strategy:
    type: Recreate
  selector:
    matchLabels:
      app: wistjet
  template:
    metadata:
      labels:
        app: wistjet
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 65532
        fsGroup: 65532 # makes the volume writable for Wistjet's user
      containers:
        - name: wistjet
          image: ghcr.io/brightclouds-be/wistjet:latest
          ports:
            - containerPort: 8080
          env:
            # The address tablets use (your ingress host, or a node address), shown during setup.
            - name: WISTJET_PUBLIC_URL
              value: "http://wistjet.thuis"
            # Host names besides IP addresses that may reach Wistjet: your ingress host.
            - name: WISTJET_HOSTS
              value: "wistjet.thuis"
          volumeMounts:
            - name: data
              mountPath: /data
          readinessProbe:
            httpGet:
              path: /api/status
              port: 8080
          livenessProbe:
            httpGet:
              path: /api/status
              port: 8080
            periodSeconds: 30
          resources:
            requests:
              cpu: 10m
              memory: 32Mi
            limits:
              memory: 512Mi # a restore holds the uploaded backup in memory
      volumes:
        - name: data
          persistentVolumeClaim:
            claimName: wistjet-data
---
apiVersion: v1
kind: Service
metadata:
  name: wistjet
spec:
  selector:
    app: wistjet
  ports:
    - port: 80
      targetPort: 8080
---
# Reaching Wistjet by name on the home network; adjust host and class to your cluster.
# Keep it on the home network: Wistjet is not meant to be exposed to the internet.
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: wistjet
spec:
  rules:
    - host: wistjet.thuis
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: wistjet
                port:
                  number: 80
```

- [ ] **Step 2: Validate it**

Run: `docker run --rm -v "$PWD/k8s:/k8s" ghcr.io/yannh/kubeconform:latest -strict -summary -kubernetes-version 1.32.0 /k8s/wistjet.yaml`
Expected: `4 resources found in 1 file - Valid: 4, Invalid: 0, Errors: 0, Skipped: 0`.

Deploying it to a real cluster is the family's own step: it changes their infrastructure, so it isn't part of this plan's automatic checks.

- [ ] **Step 3: Commit**

```bash
git add k8s
git commit -m "feat: Kubernetes example for home clusters"
```

---

### Task 4: Release workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Write the workflow**

`.github/workflows/release.yml`:
```yaml
# Publishes a release when a version tag is pushed:
#
#   git tag v0.1.0 && git push origin v0.1.0
#
# - download-ready binaries for Windows, macOS and Linux, attached to the GitHub release;
# - the Docker image ghcr.io/<owner>/wistjet for amd64 and arm64 (Raspberry Pi, most NAS).
name: release
on:
  push:
    tags: ["v*"]

permissions:
  contents: write # create the release and upload files
  packages: write # push the image to ghcr.io

jobs:
  binaries:
    strategy:
      matrix:
        include:
          - { os: ubuntu-latest, target: x86_64-unknown-linux-gnu, name: wistjet-linux-x86_64 }
          - { os: ubuntu-24.04-arm, target: aarch64-unknown-linux-gnu, name: wistjet-linux-arm64 }
          - { os: macos-latest, target: aarch64-apple-darwin, name: wistjet-macos-apple-silicon }
          - { os: macos-latest, target: x86_64-apple-darwin, name: wistjet-macos-intel }
          - { os: windows-latest, target: x86_64-pc-windows-msvc, name: wistjet-windows-x86_64 }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: web/package-lock.json
      - run: npm --prefix web ci
      - run: npm --prefix web run build
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release --locked --target ${{ matrix.target }}
      - name: Package (Linux, macOS)
        if: runner.os != 'Windows'
        run: |
          mkdir ${{ matrix.name }}
          cp target/${{ matrix.target }}/release/wistjet README.md LICENSE ${{ matrix.name }}/
          tar czf ${{ matrix.name }}.tar.gz ${{ matrix.name }}
      - name: Package (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: |
          New-Item -ItemType Directory ${{ matrix.name }}
          Copy-Item target/${{ matrix.target }}/release/wistjet.exe, README.md, LICENSE ${{ matrix.name }}/
          Compress-Archive ${{ matrix.name }} ${{ matrix.name }}.zip
      - uses: softprops/action-gh-release@v2
        with:
          files: ${{ matrix.name }}.*

  image:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/${{ github.repository_owner }}/wistjet
          tags: |
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=raw,value=latest
      # ponytail: arm64 is built under QEMU (slow, tens of minutes); split per-arch native
      # runners and merge the manifests if release time ever matters.
      - uses: docker/build-push-action@v6
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
```

- [ ] **Step 2: Lint it**

Run: `docker run --rm -v "$PWD:/repo" -w /repo rhysd/actionlint:latest -color=false; echo "exit=$?"`
Expected: no output and `exit=0` (it checks `ci.yml` too).

- [ ] **Step 3: Check the slow part locally**

Run: `docker buildx build --platform linux/amd64 -t wistjet:amd64 --load . && docker image inspect wistjet:amd64 --format '{{.Architecture}}'`
Expected: `amd64`. On an arm64 machine this runs under emulation, the same way the workflow builds arm64 on GitHub's amd64 runners. It takes a while.

The workflow itself runs for the first time when the first version tag is pushed; that is the family's call, after this plan is merged and the repository is on GitHub. The `ubuntu-24.04-arm` runner is free for public repositories.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: release binaries and Docker image on version tags"
```

---

### Task 5: README, license and screenshots

**Files:**
- Create: `README.md`, `LICENSE`, `docs/images/{child-home,study,summary,parent-deck}.png`
- Modify: `Cargo.toml`, `web/package.json`, `e2e/e2e.mjs`

- [ ] **Step 1: The license**

Run:
```bash
curl -sS -o LICENSE https://www.gnu.org/licenses/agpl-3.0.txt
shasum -a 256 LICENSE
```
Expected: `0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0`.

In `Cargo.toml`, add `license = "AGPL-3.0-only"` after `edition = "2024"`. In `web/package.json`, add `"license": "AGPL-3.0-only",` after `"version"`.

- [ ] **Step 2: Screenshots from the end-to-end test**

`e2e/e2e.mjs` becomes (new: `SCREENSHOTS`, the `shot()` helper and four calls, and a window of 1100×760):
```js
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
for (let i = 0; i < 50 && !targets.some((t) => t.type === 'page'); i++) {
  await sleep(200)
  targets = await fetch(`http://127.0.0.1:${DEVTOOLS}/json`).then((r) => r.json(), () => [])
}
const ws = new WebSocket(targets.find((t) => t.type === 'page').webSocketDebuggerUrl)
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
await shot('child-home')
await click('Start met oefenen')
check((await text()).includes('1 van 3'), 'the session starts at 1 van 3')
await key(' ')
await shot('study')
await key('ArrowLeft') // not known: comes back at the end
check((await text()).includes('2 van 4'), 'a missed card comes back at the end')
for (let i = 0; i < 3; i++) {
  await key(' ')
  await key('ArrowRight')
}
const summary = (await text()).replace(/\s+/g, ' ')
check(summary.includes('Goed gedaan!'), 'the summary appears')
await shot('summary')
check(/2 wist je 1 nog niet 1 keer herhaald/.test(summary), `the summary counts known, not known and repeats (${summary})`)

console.log('All end-to-end checks passed.')
stop(0)
```

Run: `mkdir -p docs/images && npm --prefix web run build && cargo build --release && SCREENSHOTS=docs/images node e2e/e2e.mjs target/release/wistjet`
Expected: "All end-to-end checks passed." and four PNGs in `docs/images`.

- [ ] **Step 3: The README**

`README.md`:
````markdown
# Wistjet

A simple flashcard app for families, built around the **4-box system** children already use at
school. Each child picks their profile and gets one question answered: *what do I need to practise
today?* Parents add the cards; Wistjet decides which ones come back when.

Wistjet runs on your own computer or home server. No accounts, no cloud, no ads, no tracking.
The app is in Dutch.

| Child home | Studying | Summary | Parent: a deck |
|---|---|---|---|
| ![Child home](docs/images/child-home.png) | ![Studying](docs/images/study.png) | ![Summary](docs/images/summary.png) | ![A deck in the parent area](docs/images/parent-deck.png) |

## How it works

- **Children** tap their name, see how many cards are due, and study: show the answer, then
  *Wist ik* (I knew it) or *Wist ik niet* (I didn't). Missed cards come back at the end of the
  session. Children never see boxes or dates.
- **Cards move through boxes.** In the school system, box 1 comes back after 1 day, box 2 after 2,
  box 3 after 7, box 4 after 14. Knowing a card moves it up; missing it sends it back to box 1.
  Parents can copy this system and change the boxes and days.
- **Parents**, behind a PIN, add children, subjects and decks, type or paste cards (one per line:
  `le chat - de kat`), add pictures, and see how each child is doing.
- **Backups** are one file you can download and restore, and Wistjet also makes one every day.

> **Keep Wistjet on your home network.** It is made for a family at home: there are no user
> accounts, and children use it without a password. Do not make it reachable from the internet
> (no port forwarding, no public reverse proxy).

## Install

Pick one. All three keep every card, picture and answer in one place that survives upgrades.

### On a computer (Windows, macOS, Linux)

1. Download the file for your computer from the
   [latest release](https://github.com/brightclouds-be/wistjet/releases/latest) and unpack it.
2. Start `wistjet` (`wistjet.exe` on Windows). A window opens with Wistjet's addresses, and your
   browser opens the setup.
   - **Windows** may say it protected your PC: choose *More info*, then *Run anyway*, and allow
     network access so tablets can connect.
   - **macOS** may say `wistjet` is from an unidentified developer: open *System Settings* →
     *Privacy & Security*, choose *Open Anyway* next to the message, and start it again.
3. Keep the window open while you use Wistjet; closing it stops Wistjet.

Your data is in your user's app-data folder (`~/Library/Application Support/Wistjet` on macOS,
`%APPDATA%\Wistjet\data` on Windows, `~/.local/share/wistjet` on Linux).

### With Docker (a NAS or home server)

```sh
docker run -d --name wistjet --restart unless-stopped \
  -p 8080:8080 -v wistjet-data:/data \
  -e WISTJET_PUBLIC_URL=http://192.168.1.10:8080 \
  ghcr.io/brightclouds-be/wistjet:latest
docker logs wistjet   # shows the setup code
```

Or use the [`compose.yaml`](compose.yaml) in this repository: `docker compose up -d`, then
`docker compose logs wistjet`. Replace `192.168.1.10` with your server's address. The image runs
as user id 65532 and works on amd64 and arm64 (Raspberry Pi, most NAS).

### On Kubernetes

[`k8s/wistjet.yaml`](k8s/wistjet.yaml) has a volume, a single-pod deployment (Wistjet keeps its data
in one SQLite file, so it must never run twice), a service and an ingress. Set your host name in
it, then `kubectl apply -f k8s/wistjet.yaml` and `kubectl logs deploy/wistjet` for the setup code.

## First run

The first screen asks for a family name, a PIN for the parent area, your first child, and whether
to add a small French–Dutch sample deck. When you start Wistjet on a computer, the browser it
opens already knows the **setup code**. On any other device, type the code shown in Wistjet's
window or log. This keeps someone else on your network from setting up Wistjet (and its PIN)
before you do.

At the end, a QR code shows the address for **tablets** on the same wifi. Add it to their home
screen.

## Configuration

Wistjet works without any settings. These environment variables change the defaults:

| Variable | Default | What it does |
|---|---|---|
| `WISTJET_DATA_DIR` | your app-data folder (`/data` in Docker) | Where the database lives |
| `WISTJET_PORT` | `8080` | The port Wistjet listens on |
| `WISTJET_PUBLIC_URL` | detected | The address tablets use, shown during setup; set it in Docker and Kubernetes |
| `WISTJET_HOSTS` | (none) | Extra host names that may reach Wistjet, comma-separated (e.g. a reverse-proxy name). IP addresses, `localhost`, single-word names and `.local` names always work |
| `WISTJET_BACKUP_DIR` | `backups` in the data folder | Where daily backups go, e.g. a NAS share |
| `WISTJET_BACKUP_KEEP` | `14` | How many daily backups to keep |
| `WISTJET_OPEN_BROWSER` | `1` (`0` in Docker) | `0` stops Wistjet from opening a browser at start |

## Backups

- **Download a backup:** parent area → *Instellingen* → *Download back-up*. It is one file with
  everything: children, decks, cards, pictures, progress and settings.
- **Restore:** *Instellingen* → *Back-up terugzetten…*. After you confirm, it replaces everything.
  A backup from an older Wistjet works in a newer one. A file that isn't a Wistjet backup is
  refused, and nothing changes.
- **Daily backups:** Wistjet writes `wistjet-YYYY-MM-DD.db` to its backup folder every day and keeps
  the newest 14. Point `WISTJET_BACKUP_DIR` at another disk or a NAS share, so a broken disk doesn't
  take your backups with it.

## Upgrading

Download the new release and start it instead of the old one, or `docker pull` the image and
recreate the container. Wistjet updates its data automatically the first time the new version
starts. To be safe, download a backup before you upgrade.

## Development

You need Rust (stable) and Node 22.

```sh
npm --prefix web ci && npm --prefix web run build   # the web app, embedded in the binary
cargo run                                            # http://localhost:8080
npm --prefix web run dev                             # UI with hot reload, API from cargo run
```

Checks, as run by CI:

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
npm --prefix web run check && npm --prefix web test
cargo build --release && node e2e/e2e.mjs target/release/wistjet   # needs Chrome
```

`SCREENSHOTS=docs/images node e2e/e2e.mjs target/release/wistjet` also refreshes the screenshots
above. The design and the plans it was built from are in [`docs/superpowers`](docs/superpowers).

To release, push a version tag (`git tag v0.1.0 && git push origin v0.1.0`). The release workflow
builds the downloads and the Docker image.

## License

Wistjet is free software under the [GNU Affero General Public License v3.0](LICENSE). You may use,
change and share it. If you run a changed version for others over a network, you must offer them
its source code.
````

- [ ] **Step 4: Check everything**

Run:
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
npm --prefix web run check && npm --prefix web test
node e2e/e2e.mjs target/release/wistjet
```
Expected: all green. Open `README.md` in a Markdown preview: the screenshots show, and the links point to `compose.yaml`, `k8s/wistjet.yaml`, `LICENSE` and `docs/superpowers`.

- [ ] **Step 5: Commit**

```bash
git add README.md LICENSE docs/images Cargo.toml web/package.json e2e/e2e.mjs
git commit -m "docs: README, AGPL-3.0 license and screenshots"
```

---

## Done when

- The Docker image builds and passes the Task 2 run-through; the manifest validates; both workflows lint clean; the README, license and screenshots are in place; all earlier checks stay green.
- After merging: the family pushes the repository and a first `v0.1.0` tag, which runs the release workflow for real.
- Spec coverage: §9.1 complete (one-command install on a computer and a home cluster, documented; data in one configurable place; automatic upgrades, which were already built). With this plan, every section of the spec is implemented.
