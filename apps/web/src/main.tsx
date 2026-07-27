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
if (import.meta.env.PROD && 'serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch((err) => {
      console.error('[sustena] service worker registration failed:', err)
    })
  })
}
