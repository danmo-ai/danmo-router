import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Split deploy: user API defaults to :3002. Override with VITE_API_PROXY.
const apiProxy = process.env.VITE_API_PROXY || 'http://127.0.0.1:3002'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5174,
    proxy: {
      '/api': apiProxy,
    },
  },
})
