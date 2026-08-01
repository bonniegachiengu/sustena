import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './styles/globals.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)

// PWA service worker — production builds only. Registering it under `vite
// dev` would let it intercept and cache HMR/module requests, which fights
// the dev server rather than helping it; import.meta.env.PROD is false
// there and true in the built dist/ that's actually served publicly.
//
// The ?v=<build hash> query string exists specifically to defeat an
// intermediary CDN cache (found live 1 Aug 2026: Cloudflare's default edge
// cache served a stale /sw.js from its stable URL for its full max-age
// regardless of how many times the origin redeployed — the origin now also
// sends Cache-Control: no-cache/no-store on /sw.js itself, but that only
// takes effect once an already-cached edge copy expires or is purged). A
// different query string is a different cache key, so every real deploy
// forces a genuinely fresh fetch through the CDN immediately, not just
// eventually. Doesn't change the service worker's scope — that's derived
// from the script URL's directory, which the query string doesn't touch.
if (import.meta.env.PROD && 'serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register(`/sw.js?v=${__SW_BUILD_HASH__}`).catch((err) => {
      console.error('[sustena] service worker registration failed:', err)
    })
  })
}
