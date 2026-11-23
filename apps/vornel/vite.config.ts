import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri 移动端开发时会设置这个变量，让手机能访问开发服务器
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    // 界面用到 CSS color-mix，需要 Safari 16.2+（macOS 13+ / iOS 16.4+）与 WebView2
    target: ['safari16', 'chrome111'],
    assetsInlineLimit: 0,
    // 桌面应用从本地加载，xterm.js 打成一个较大的包没有问题
    chunkSizeWarningLimit: 1200,
  },
});
