import path from 'path';
import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, path.resolve(__dirname, '..'), '');
  const apiPort = env.API_PORT || '3000';
  const devTarget = env.DEV_API_TARGET || `http://127.0.0.1:${apiPort}`;
  const wsTarget = devTarget.replace(/^http/, 'ws');

  return {
    plugins: [react()],
    envDir: '..',
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      },
    },
    server: {
      port: 5473,
      proxy: {
        '/api': { target: devTarget, changeOrigin: true },
        '/ws': { target: wsTarget, ws: true, changeOrigin: true },
        '/sub': { target: devTarget, changeOrigin: true },
      },
    },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      chunkSizeWarningLimit: 600,
      rollupOptions: {
        output: {
          manualChunks: {
            'react-vendor': ['react', 'react-dom', 'react-router-dom'],
            'query-state': ['@tanstack/react-query', 'axios', 'zustand'],
            charts: ['recharts'],
            radix: [
              '@radix-ui/react-checkbox',
              '@radix-ui/react-dialog',
              '@radix-ui/react-dropdown-menu',
              '@radix-ui/react-label',
              '@radix-ui/react-popover',
              '@radix-ui/react-select',
              '@radix-ui/react-separator',
              '@radix-ui/react-slot',
              '@radix-ui/react-switch',
              '@radix-ui/react-tabs',
              '@radix-ui/react-toast',
              '@radix-ui/react-tooltip',
            ],
          },
        },
      },
    },
  };
});
