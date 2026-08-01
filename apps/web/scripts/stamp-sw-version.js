/**
 * apps/web/scripts/stamp-sw-version.js
 *
 * Runs as the last step of `npm run build`, AFTER vite has already copied
 * public/sw.js into dist/sw.js verbatim. Rewrites dist/sw.js's CACHE_VERSION
 * to include the current git commit — never touches the git-tracked source
 * file (public/sw.js), so the repo stays clean; only the build OUTPUT
 * (dist/, already gitignored) carries the stamp.
 *
 * Why this exists: an installed PWA is a long-lived, offline-capable client.
 * Its service worker only re-installs when the browser detects the SW
 * SCRIPT'S BYTES have changed. Before this script existed, sw.js's
 * CACHE_VERSION was a static string ('sustena-shell-v1') that never
 * changed between deploys — so an installed client could go on serving a
 * shell it cached weeks ago (old JS bundle, old asset hashes, calling
 * whatever API routes existed at THAT build) indefinitely, even though the
 * server had long since moved on. This is the concrete mechanism behind a
 * phone showing "no curated widgets" / 404ing a real, currently-deployed
 * route: not a backend bug, a stuck offline-first client. Stamping the
 * commit hash in on every build means every real deploy is also a forced
 * service-worker update for every installed client, next time they get a
 * moment of connectivity.
 */
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const swPath = join(__dirname, '..', 'dist', 'sw.js');

if (!existsSync(swPath)) {
  console.warn(`stamp-sw-version: ${swPath} not found (no dist/sw.js built) — skipping.`);
  process.exit(0);
}

let commit = 'unknown';
try {
  commit = execSync('git rev-parse --short=12 HEAD', { cwd: join(__dirname, '..'), encoding: 'utf8' }).trim();
} catch (e) {
  console.warn('stamp-sw-version: could not resolve git commit, using "unknown" —', e.message);
}

const original = readFileSync(swPath, 'utf8');
const stamped = original.replace(
  /const CACHE_VERSION = ['"][^'"]*['"];/,
  `const CACHE_VERSION = 'sustena-shell-${commit}';`
);

if (stamped === original) {
  console.warn('stamp-sw-version: CACHE_VERSION pattern not found in dist/sw.js — nothing stamped. Check public/sw.js still declares `const CACHE_VERSION = \'...\';` verbatim.');
  process.exit(1);
}

writeFileSync(swPath, stamped, 'utf8');
console.log(`stamp-sw-version: dist/sw.js CACHE_VERSION -> sustena-shell-${commit}`);
