import React from 'react'
import ReactDOM from 'react-dom/client'
import { Capacitor } from '@capacitor/core'
import App from './App'
import './styles/globals.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)

// PWA service worker — production BROWSER builds only. The native Android
// shell (apps/web/scripts/../android, built via `npm run build:capacitor`)
// bundles the web assets directly into the app package, so there is no
// "installed shell that can go stale between deploys" for a service worker
// to defend against there — Capacitor.isNativePlatform() is true only
// inside that wrapped app, never in a normal browser tab, PROD or dev.
// Registering it there anyway would just be a redundant caching layer with
// no server behind it to talk to for updates.
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
if (import.meta.env.PROD && !Capacitor.isNativePlatform() && 'serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register(`/sw.js?v=${__SW_BUILD_HASH__}`).catch((err) => {
      console.error('[sustena] service worker registration failed:', err)
    })
  })
}
