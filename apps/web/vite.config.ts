import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { execSync } from 'node:child_process'

// Resolved once, at build time (not runtime) -- used only to cache-bust the
// service worker registration URL (see main.tsx's __SW_BUILD_HASH__ use and
// scripts/stamp-sw-version.js's own comment for why: an intermediary CDN
// cache can hold a stale /sw.js at its stable URL indefinitely regardless
// of origin deploys, so a fresh query string forces a genuinely new cache
// key on every build). 'unknown' is a safe fallback for an environment with
// no .git (a packaged deploy) -- must never fail the build over this.
function resolveBuildHash(): string {
  try {
    return execSync('git rev-parse --short=12 HEAD', { cwd: process.cwd() }).toString().trim()
  } catch {
    return 'unknown'
  }
}

export default defineConfig({
  plugins: [react()],
  define: {
    __SW_BUILD_HASH__: JSON.stringify(resolveBuildHash()),
  },
  server: {
    port: 3000,
    proxy: {
      '/api':     { target: 'http://localhost:9000', changeOrigin: true, ws: false },
      '/devui':   { target: 'http://localhost:9000', changeOrigin: true, ws: true  },
      '/orchie':  { target: 'http://localhost:9000', changeOrigin: true, ws: false },
      '/webhook': { target: 'http://localhost:9000', changeOrigin: true, ws: false },
      '/seed':    { target: 'http://localhost:9000', changeOrigin: true, ws: false },
    }
  },
  build: {
    outDir: 'dist'
  }
})
