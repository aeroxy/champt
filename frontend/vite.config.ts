import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// In development the API is the local Rust server: `cargo run --bin dev`.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5176,
    proxy: { '/api': 'http://127.0.0.1:8787' },
  },
})
