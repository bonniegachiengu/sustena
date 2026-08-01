/// <reference types="vite/client" />

// Injected by vite.config.ts's `define` at build time — the current git
// commit, short form. Used only to cache-bust the service worker
// registration URL; see main.tsx.
declare const __SW_BUILD_HASH__: string;
