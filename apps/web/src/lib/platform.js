/**
 * apps/web/src/lib/platform.js
 *
 * Detects whether the app is running inside a packaged native shell
 * (Capacitor on Android, Tauri on desktop) as opposed to a real browser
 * tab -- dev, or the hosted PWA at sustena.vyybandasky.online. Both
 * matter the same way in a few places: neither has a same-origin browser
 * session to inherit (LoginGate needs a real form instead of assuming a
 * token already exists), and a "go to sustena" web-navigation link makes
 * no sense inside either (there's no separate "web app" to go to from
 * inside a packaged app -- App.tsx's own native redirects would just
 * bounce it straight back).
 */
import { Capacitor } from '@capacitor/core';

/**
 * True inside ANY packaged native shell. Tauri injects
 * `window.__TAURI_INTERNALS__` into its webview -- the documented,
 * stable way to detect a Tauri runtime without adding the
 * `@tauri-apps/api` package just for this one check.
 */
export function isNativeShell() {
  if (Capacitor.isNativePlatform()) return true;
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) return true;
  return false;
}

/** True specifically inside the Tauri desktop shell (Sustena Studio). */
export function isTauri() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
