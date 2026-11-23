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
    // macOS 12+ / iOS 15+ 的 WKWebView、Windows 的 WebView2 都支持
    target: ['safari15', 'chrome110'],
    assetsInlineLimit: 0,
    // 桌面应用从本地加载，xterm.js 打成一个较大的包没有问题
    chunkSizeWarningLimit: 1200,
  },
});
