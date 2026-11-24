<script lang="ts">
  // Windows 无边框窗口的“最小化 / 最大化 / 关闭”按钮（macOS 用系统红绿灯）
  import { getCurrentWindow } from '@tauri-apps/api/window';

  const win = getCurrentWindow();
  let maximized = $state(false);

  $effect(() => {
    void win.isMaximized().then((m) => (maximized = m));
    const un = win.onResized(() => void win.isMaximized().then((m) => (maximized = m)));
    return () => void un.then((f) => f());
  });
</script>

<div class="wc">
  <button type="button" aria-label="最小化" onclick={() => win.minimize()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" /></svg>
  </button>
  <button type="button" aria-label={maximized ? '还原' : '最大化'} onclick={() => win.toggleMaximize()}>
    {#if maximized}
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z" /></svg>
    {:else}
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5h9v9h-9z" /></svg>
    {/if}
  </button>
  <button class="close" type="button" aria-label="关闭" onclick={() => win.close()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10" /></svg>
  </button>
</div>

<style>
  .wc {
    display: flex;
    align-self: stretch;
    flex: none;
  }
  button {
    width: 46px;
    display: grid;
    place-items: center;
    border: 0;
    background: none;
    color: var(--mut);
    transition:
      background 120ms ease-out,
      color 120ms ease-out;
  }
  button:hover {
    background: var(--sel);
    color: var(--fg);
  }
  .close:hover {
    background: #c42b1c;
    color: #fff;
  }
  path {
    fill: none;
    stroke: currentColor;
    stroke-width: 1;
  }
</style>
