<script lang="ts">
  import type { Host } from '../lib/api';
  import type { PaneStatus } from './TerminalPane.svelte';
  import { loadCurrentVersion, updater } from '../lib/updater.svelte';

  let { host, status }: { host: Host | null; status: PaneStatus | null } = $props();

  const label: Record<string, string> = {
    auth: '等待认证',
    connecting: '正在连接',
    connected: '已连接',
    closed: '已断开',
    error: '连接失败',
  };

  // 显示实际运行的版本（来自 tauri.conf.json），更新后随之变化
  void loadCurrentVersion();
</script>

<footer class="status">
  {#if host && status}
    <span class="st {status.state}">● {label[status.state]}</span>
    <span>{host.username ? `${host.username}@` : ''}{host.address}:{host.port}</span>
    {#if host.protocol === 'ssh' || host.protocol === 'mosh'}
      <span>{host.auth.type === 'password' ? '密码' : host.auth.type === 'agent' ? 'SSH Agent' : '私钥'}</span>
    {/if}
    <span class="sp"></span>
    <span>UTF-8</span>
    {#if status.cols && status.rows}<span>{status.cols}×{status.rows}</span>{/if}
  {:else}
    <span>就绪</span>
    <span class="sp"></span>
    {#if updater.current}<span>Vornel {updater.current}</span>{/if}
  {/if}
</footer>

<style>
  .status {
    height: 27px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 12px;
    border-top: 1px solid var(--line);
    font: 11.5px var(--f-mono);
    color: var(--mut);
    white-space: nowrap;
    overflow: hidden;
  }
  .sp {
    flex: 1;
  }
  .st.connected {
    color: var(--ok);
  }
  .st.connecting,
  .st.auth {
    color: var(--warn);
  }
  .st.error {
    color: var(--err);
  }
</style>
