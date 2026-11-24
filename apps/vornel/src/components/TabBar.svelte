<script lang="ts" module>
  import type { PaneState } from './TerminalPane.svelte';

  export interface TabInfo {
    key: string;
    title: string;
    state: PaneState;
    /** 处于分屏中 */
    split?: boolean;
  }
</script>

<script lang="ts">
  import { flip } from 'svelte/animate';
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    tabs,
    activeKey,
    onselect,
    onclose,
    oncontext,
    onreorder,
    onrename,
    renaming = $bindable(null),
  }: {
    tabs: TabInfo[];
    activeKey: string | null;
    onselect: (key: string) => void;
    onclose: (key: string) => void;
    /** 标签右键菜单 */
    oncontext?: (e: MouseEvent, key: string) => void;
    /** 拖动调整顺序：把 from 位置的标签移到 to */
    onreorder?: (from: number, to: number) => void;
    /** 临时重命名（空字符串表示恢复主机名） */
    onrename?: (key: string, title: string) => void;
    /** 正在重命名的标签（由右键菜单打开） */
    renaming?: string | null;
  } = $props();

  // ---- 拖动排序：按下后移动超过 5px 才算拖动，拖动时实时交换位置（配合 flip 动画）
  let tabsEl = $state<HTMLDivElement>();
  let drag = $state<{ key: string; startX: number; moved: boolean } | null>(null);
  let suppressClick = false;

  function onDown(e: PointerEvent, key: string) {
    if (e.button !== 0 || renaming) return;
    drag = { key, startX: e.clientX, moved: false };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onMove(e: PointerEvent) {
    if (!drag || !tabsEl) return;
    if (!drag.moved && Math.abs(e.clientX - drag.startX) < 5) return;
    drag.moved = true;
    const from = tabs.findIndex((t) => t.key === drag!.key);
    const els = [...tabsEl.querySelectorAll<HTMLElement>(':scope > .tab')];
    let to = from;
    els.forEach((el, i) => {
      if (i === from) return;
      const r = el.getBoundingClientRect();
      const mid = r.left + r.width / 2;
      if (i > from && e.clientX > mid) to = i;
      if (i < from && e.clientX < mid && to === from) to = i;
    });
    if (to !== from) onreorder?.(from, to);
  }
  function onUp() {
    if (drag?.moved) suppressClick = true;
    drag = null;
  }

  // ---- 临时重命名
  let draft = $state('');
  let input = $state<HTMLInputElement>();
  $effect(() => {
    if (!renaming) return;
    draft = tabs.find((t) => t.key === renaming)?.title ?? '';
    void tick().then(() => {
      input?.focus();
      input?.select();
    });
  });
  function commit() {
    if (renaming) onrename?.(renaming, draft.trim());
    renaming = null;
  }

  function dotClass(s: PaneState) {
    return s === 'connected' ? 'ok' : s === 'connecting' ? 'busy' : s === 'error' ? 'err' : '';
  }
</script>

<div class="tb" data-tauri-drag-region>
  <div class="tabs" role="tablist" aria-label="会话" bind:this={tabsEl}>
    {#each tabs as t (t.key)}
      <div class="tab" class:on={t.key === activeKey} class:split={t.split} class:dragging={drag?.moved && drag.key === t.key} animate:flip={{ duration: 160 }}>
        {#if renaming === t.key}
          <span class="label">
            <span class="dot {dotClass(t.state)}"></span>
            <input
              class="rename"
              bind:this={input}
              bind:value={draft}
              aria-label="重命名标签"
              onkeydown={(e) => {
                if (e.key === 'Enter') commit();
                else if (e.key === 'Escape') renaming = null;
                e.stopPropagation();
              }}
              onblur={commit}
            />
          </span>
        {:else}
          <button
            class="label"
            type="button"
            role="tab"
            aria-selected={t.key === activeKey}
            title="拖动可调整顺序，双击或右键可重命名"
            onpointerdown={(e) => onDown(e, t.key)}
            onpointermove={onMove}
            onpointerup={onUp}
            onpointercancel={onUp}
            onclick={() => {
              if (suppressClick) return void (suppressClick = false);
              onselect(t.key);
            }}
            ondblclick={() => (renaming = t.key)}
            onauxclick={(e) => e.button === 1 && onclose(t.key)}
            oncontextmenu={(e) => oncontext?.(e, t.key)}
          >
            <span class="dot {dotClass(t.state)}"></span>
            <span class="name">{t.title}</span>
          </button>
        {/if}
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
    animation: tab-in 200ms var(--ease) both;
  }
  .tab.dragging {
    z-index: 1;
    background: var(--raise);
    box-shadow: 0 6px 16px -8px rgba(0, 0, 0, 0.5);
    opacity: 0.92;
    cursor: grabbing;
  }
  .rename {
    width: 120px;
    height: 21px;
    padding: 0 6px;
    border: 0;
    border-radius: 5px;
    outline: 1.5px solid var(--acc);
    background: var(--win);
    color: var(--fg);
    font: 12.5px var(--f-mono);
  }
  /* 分屏中的标签：底部一条强调色细线 */
  .tab.split {
    box-shadow: inset 0 -2px 0 color-mix(in srgb, var(--acc) 70%, transparent);
  }
  @keyframes tab-in {
    from {
      opacity: 0;
      transform: translateX(-8px) scale(0.96);
    }
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
