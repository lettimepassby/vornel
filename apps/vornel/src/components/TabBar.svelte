<script lang="ts" module>
  import type { PaneState } from './TerminalPane.svelte';

  export interface TabInfo {
    key: string;
    title: string;
    state: PaneState;
  }
</script>

<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    tabs,
    activeKey,
    onselect,
    onclose,
  }: {
    tabs: TabInfo[];
    activeKey: string | null;
    onselect: (key: string) => void;
    onclose: (key: string) => void;
  } = $props();

  function dotClass(s: PaneState) {
    return s === 'connected' ? 'ok' : s === 'connecting' ? 'busy' : s === 'error' ? 'err' : '';
  }
</script>

<div class="tb" data-tauri-drag-region>
  <div class="tabs" role="tablist" aria-label="会话">
    {#each tabs as t (t.key)}
      <div class="tab" class:on={t.key === activeKey}>
        <button
          class="label"
          type="button"
          role="tab"
          aria-selected={t.key === activeKey}
          onclick={() => onselect(t.key)}
          onauxclick={(e) => e.button === 1 && onclose(t.key)}
        >
          <span class="dot {dotClass(t.state)}"></span>
          <span class="name">{t.title}</span>
        </button>
        <button class="x" type="button" aria-label="关闭 {t.title}" onclick={() => onclose(t.key)}>
          <Icon name="close" size={13} />
        </button>
      </div>
    {/each}
  </div>
  <span class="sp" data-tauri-drag-region></span>
</div>

<style>
  .tb {
    height: 46px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 8px 0 10px;
    border-bottom: 1px solid var(--line);
  }
  .tabs {
    display: flex;
    gap: 4px;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    display: flex;
    align-items: center;
    height: 29px;
    flex: none;
    border-radius: 7px;
    color: var(--mut);
    transition: background 120ms ease-out;
  }
  .tab:hover {
    background: var(--sel);
  }
  .tab.on {
    background: var(--raise);
    color: var(--fg);
    box-shadow: 0 0 0 1px var(--line);
  }
  .label {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 100%;
    padding: 0 4px 0 10px;
    border: 0;
    background: none;
    font: 12.5px var(--f-mono);
  }
  .name {
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .x {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-right: 4px;
    border: 0;
    border-radius: 5px;
    background: none;
    color: var(--mut);
    opacity: 0;
  }
  .tab:hover .x,
  .tab.on .x,
  .x:focus-visible {
    opacity: 1;
  }
  .x:hover {
    background: var(--sel);
    color: var(--fg);
  }
  .sp {
    flex: 1;
    align-self: stretch;
  }
</style>
