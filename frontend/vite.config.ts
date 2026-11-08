import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// In development the API is the local Rust server: `cargo run --bin dev`.
// It also serves a thread's link (/t/<id>) as a page with a link preview,
// built from this server's index.html, as it does on Vercel.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5176,
    proxy: {
      '/api': 'http://127.0.0.1:8787',
      // Keeping the Host header tells the API to fetch index.html from here.
      '^/t/[^/]+/?$': { target: 'http://127.0.0.1:8787', changeOrigin: false },
    },
  },
})
