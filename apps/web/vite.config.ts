import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
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
