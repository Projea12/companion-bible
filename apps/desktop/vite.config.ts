import { defineConfig } from 'vite';
import { resolve } from 'path';
import react from '@vitejs/plugin-react';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  // Vite options tailored for Tauri development
  clearScreen: false,
  server: {
    host: host || false,
    port: 1420,
    strictPort: true,
    // Required for mobile development; harmless for desktop
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Tell Vite to ignore watching `src-tauri`
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    rollupOptions: {
      input: {
        // Operator window — primary display
        operator: resolve(__dirname, 'index.html'),
        // Congregation window — secondary / projected display
        congregation: resolve(__dirname, 'congregation.html'),
      },
    },
    // Tauri requires a minChunkSize that won't split too aggressively
    target: ['es2021', 'chrome100', 'safari13'],
    minify: !process.env.TAURI_DEBUG,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
