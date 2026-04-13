import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import markdown from './plugins/markdown.js'

export default defineConfig({
  plugins: [react(), tailwindcss(), markdown()],
  base: '/',
  server: {
    host: '0.0.0.0',
  },
})
