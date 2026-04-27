import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api':  { target: 'http://localhost:3000', changeOrigin: true },
      '/auth': { target: 'http://localhost:3000', changeOrigin: true },
    },
  },
  build: {
    chunkSizeWarningLimit: 600,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules/three'))          return 'vendor-three'
          if (id.includes('node_modules/satellite.js'))   return 'vendor-satjs'
          if (id.includes('node_modules/i18next') ||
              id.includes('node_modules/react-i18next'))  return 'vendor-i18n'
          if (id.includes('node_modules/react') ||
              id.includes('node_modules/react-dom') ||
              id.includes('node_modules/react-router'))   return 'vendor-react'
        },
      },
    },
  },
})
