import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const root = dirname(fileURLToPath(import.meta.url))

// Split deploy: user API defaults to :3002. Override with VITE_API_PROXY.
const apiProxy = process.env.VITE_API_PROXY || 'http://127.0.0.1:3002'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': resolve(root, 'src'),
    },
  },
  server: {
    port: 5174,
    proxy: {
      '/api': apiProxy,
    },
  },
})
