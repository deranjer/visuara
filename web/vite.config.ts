import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Dev server proxies /api/v1 and /ws to a locally running
// `cargo run -p visuara-signaling` (default port 8080), so `npm run dev`
// gives hot-reload against a live backend without CORS/cookie hassles.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api/v1': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://localhost:8080',
        ws: true,
      },
    },
  },
  build: {
    outDir: 'dist',
  },
});
