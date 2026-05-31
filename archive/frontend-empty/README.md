# Sustena XII — Frontend

## Phase 1 (Now) — HTMX + Jinja2

No npm install needed. Phase 1 is server-rendered HTML served by FastAPI.
All JS (HTMX, Alpine.js) loads from CDN links inside the HTML templates.
Templates live in `backend/sustena/templates/`.
Static assets (CSS, service worker) live in `backend/sustena/static/`.

To run the Phase 1 UI: just start the backend server.

## Phase 2 (React Native mobile app)

When you're ready to build the mobile app, install from `package.json`:

```powershell
cd frontend\
npm install
```

Then to start the Expo dev server:

```powershell
npx expo start
```
