<script lang="ts">
  // 全局右键菜单：键盘 ↑↓ 选择、→ 打开子菜单、Enter 执行、Esc 关闭；超出窗口时自动翻转位置
  import { tick } from 'svelte';
  import { closeMenu, ctxMenu, type MenuItem } from '../lib/contextmenu.svelte';
  import Icon from './Icon.svelte';
  import ContextMenu from './ContextMenu.svelte';

  let { items = null, x = 0, y = 0, nested = false }: { items?: MenuItem[] | null; x?: number; y?: number; nested?: boolean } = $props();

  const list = $derived(items ?? ctxMenu.items);
  const open = $derived(nested || ctxMenu.open);
  let el = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });
  let active = $state(-1);
  let sub = $state<{ index: number; x: number; y: number } | null>(null);

  $effect(() => {
    if (!open) return;
    const px = nested ? x : ctxMenu.x;
    const py = nested ? y : ctxMenu.y;
    active = -1;
    sub = null;
    pos = { x: px, y: py };
    void tick().then(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      const W = window.innerWidth;
      const H = window.innerHeight;
      pos = {
        x: px + r.width > W - 6 ? Math.max(6, nested ? px - r.width - (el.parentElement?.getBoundingClientRect().width ?? 0) + 4 : px - r.width) : px,
        y: py + r.height > H - 6 ? Math.max(6, H - r.height - 6) : py,
      };
      if (!nested) el.focus();
    });
  });

  function run(it: MenuItem) {
    if (it === 'sep' || it.disabled) return;
    if (it.submenu) return;
    closeMenu();
    void it.action?.();
  }

  function enabled(i: number) {
    const it = list[i];
    return it && it !== 'sep' && !it.disabled;
  }

  function move(d: number) {
    let i = active;
    for (let n = 0; n < list.length; n++) {
      i = (i + d + list.length) % list.length;
      if (enabled(i)) break;
    }
    active = i;
  }

  function openSub(i: number, target: HTMLElement) {
    const it = list[i];
    if (it === 'sep' || !it?.submenu || it.disabled) {
      sub = null;
      return;
    }
    const r = target.getBoundingClientRect();
    sub = { index: i, x: r.right - 2, y: r.top - 5 };
  }

  function onkey(e: KeyboardEvent) {
    if (nested) return;
    if (e.key === 'Escape') closeMenu();
    else if (e.key === 'ArrowDown') move(1);
    else if (e.key === 'ArrowUp') move(-1);
    else if (e.key === 'ArrowRight' && active >= 0) openSub(active, el!.querySelectorAll<HTMLElement>(':scope > .mi')[active]);
    else if (e.key === 'Enter' && active >= 0) run(list[active]);
    else return;
    e.preventDefault();
  }
</script>

<svelte:window
  onmousedown={(e) => !nested && ctxMenu.open && !(e.target as HTMLElement).closest?.('.cm') && closeMenu()}
  onblur={() => !nested && closeMenu()}
  onresize={() => !nested && closeMenu()}
/>

{#if open}
  <div
    bind:this={el}
    class="cm"
    class:nested
    role="menu"
    tabindex="-1"
    style:left="{pos.x}px"
    style:top="{pos.y}px"
    onkeydown={onkey}
    oncontextmenu={(e) => e.preventDefault()}
  >
    {#each list as it, i (i)}
      {#if it === 'sep'}
        <div class="sep" role="separator"></div>
      {:else}
        <button
          class="mi"
          class:danger={it.danger}
          class:on={active === i}
          type="button"
          role="menuitem"
          disabled={it.disabled}
          aria-haspopup={it.submenu ? 'menu' : undefined}
          onmouseenter={(e) => {
            active = i;
            openSub(i, e.currentTarget);
          }}
          onclick={() => run(it)}
        >
          <span class="ic">{#if it.icon}<Icon name={it.icon} size={14} />{/if}</span>
          <span class="lb">{it.label}</span>
          {#if it.shortcut}<kbd>{it.shortcut}</kbd>{/if}
          {#if it.submenu}<span class="arrow"><Icon name="chevron" size={13} /></span>{/if}
        </button>
      {/if}
    {/each}
    {#if sub}
      {@const s = list[sub.index]}
      {#if s !== 'sep' && s.submenu}
        <ContextMenu items={s.submenu} x={sub.x} y={sub.y} nested />
      {/if}
    {/if}
  </div>
{/if}

<style>
  .cm {
    position: fixed;
    z-index: 1000;
    min-width: 200px;
    max-width: 320px;
    max-height: calc(100vh - 12px);
    overflow-y: auto;
    padding: 5px;
    border-radius: 10px;
    background: var(--raise);
    box-shadow:
      0 0 0 1px var(--line2),
      0 12px 32px -8px rgba(0, 0, 0, 0.45);
    outline: none;
    transform-origin: top left;
    animation: cm-in 120ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .cm.nested {
    animation-duration: 90ms;
  }
  @keyframes cm-in {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(-3px);
    }
  }
  .mi {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    height: 30px;
    padding: 0 10px 0 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--fg);
    font: 13px var(--f-ui);
    text-align: left;
    cursor: default;
  }
  .mi.on:not(:disabled) {
    background: var(--acc);
    color: #fff;
  }
  .mi:disabled {
    color: var(--dim);
  }
  .mi.danger:not(:disabled) {
    color: var(--err);
  }
  .mi.danger.on:not(:disabled) {
    background: var(--err);
    color: #fff;
  }
  .ic {
    width: 15px;
    display: inline-flex;
    opacity: 0.85;
  }
  .lb {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  kbd {
    font: 11px var(--f-mono);
    opacity: 0.6;
  }
  .arrow {
    display: inline-flex;
    opacity: 0.7;
  }
  .sep {
    height: 1px;
    margin: 5px 6px;
    background: var(--line);
  }
</style>
