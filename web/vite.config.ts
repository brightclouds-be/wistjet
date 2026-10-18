import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// `npm run dev` serves the UI with hot reload and forwards API calls to `cargo run`.
export default defineConfig({
  plugins: [svelte()],
  server: { proxy: { '/api': 'http://localhost:8080' } },
})
