/**
 * public/sw.js — Sustena PWA service worker.
 *
 * Job: make the app SHELL (HTML/JS/CSS/icons/manifest — everything that
 * makes the UI paint) load instantly and survive being offline. It does
 * NOT make sustain data available offline, and it must never make stale
 * data look live — every /api, /devui, /orchie, /webhook, /seed request
 * goes straight to the network, uncached, always. If that fetch fails,
 * the app's own existing UI (e.g. MonitorPanel's `offline` state, "OFFLINE
 * · LAST KNOWN DATA") is what tells the human the truth — this file's
 * only job is making sure that UI can load in the first place.
 *
 * Strategy:
 *   - navigation requests (the HTML page)  → network-first, cached-shell
 *     fallback when offline, static offline.html as the last resort if
 *     nothing is cached yet (e.g. the very first offline visit ever).
 *   - same-origin static assets (/assets/*, /icons/*, manifest, fonts)
 *     → cache-first. Safe because Vite content-hashes every built asset
 *     filename — a cache hit is always byte-identical to what a fresh
 *     build would serve, never stale.
 *   - API/data routes → network-only, never touched by this file.
 */

const CACHE_VERSION = 'sustena-shell-v1';
const OFFLINE_URL = '/offline.html';

// Never cache — these must always hit the real server so the app's own
// honest offline/online state stays truthful.
const NEVER_CACHE_PREFIXES = ['/api/', '/devui/', '/orchie/', '/webhook/', '/seed/'];

const PRECACHE_URLS = [
  '/',
  '/manifest.webmanifest',
  OFFLINE_URL,
  '/icons/icon-192.png',
  '/icons/icon-512.png',
  '/icons/apple-touch-icon.png',
  '/icons/favicon-32.png',
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_VERSION)
      .then((cache) => cache.addAll(PRECACHE_URLS))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(
        keys.filter((k) => k !== CACHE_VERSION).map((k) => caches.delete(k))
      ))
      .then(() => self.clients.claim())
  );
});

function isNeverCache(url) {
  return NEVER_CACHE_PREFIXES.some((p) => url.pathname.startsWith(p));
}

self.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return; // never intercept mutating calls

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return; // cross-origin (fonts CDN etc.) — let the browser handle it natively

  if (isNeverCache(url)) return; // network-only, no interception at all

  // Navigation (HTML page loads / route changes in the SPA shell)
  if (request.mode === 'navigate') {
    event.respondWith(
      fetch(request)
        .then((response) => {
          const copy = response.clone();
          caches.open(CACHE_VERSION).then((cache) => cache.put('/', copy));
          return response;
        })
        .catch(() =>
          caches.match('/').then((cached) => cached || caches.match(OFFLINE_URL))
        )
    );
    return;
  }

  // Static assets — cache-first, populate cache on first fetch.
  event.respondWith(
    caches.match(request).then((cached) => {
      if (cached) return cached;
      return fetch(request).then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(CACHE_VERSION).then((cache) => cache.put(request, copy));
        }
        return response;
      }).catch(() => cached); // no network, no cache — let it fail naturally (e.g. a font)
    })
  );
});
