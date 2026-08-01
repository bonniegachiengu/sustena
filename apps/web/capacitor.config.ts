import type { CapacitorConfig } from '@capacitor/cli';

// No `server.url` — the app bundles apps/web/dist LOCALLY into the APK
// (Android's own app-update mechanism is what keeps the shell current, not
// a hosted URL or any cache-invalidation logic). Build with
// `npm run build:capacitor` first so dist/ carries the absolute
// VITE_API_BASE the app needs (see .env.capacitor) — a plain `npm run
// build` output is for the hosted web app and assumes a same-origin API,
// which the local-bundle app doesn't have.
const config: CapacitorConfig = {
  appId: 'online.vyybandasky.sustena',
  appName: 'Sustena Orchie',
  webDir: 'dist',
  backgroundColor: '#0f0f0f', // matches --bg-base / manifest.webmanifest's theme_color
  android: {
    backgroundColor: '#0f0f0f',
  },
};

export default config;
