<script lang="ts">
  import { flip } from 'svelte/animate';
  import { fade } from 'svelte/transition';
  import type { Host } from '../lib/api';
  import Icon from '../components/Icon.svelte';
  import Logo from '../components/Logo.svelte';
  import { osIcon } from '../lib/os-icons';

  let {
    hosts,
    connected,
    onconnect,
    onedit,
    onnew,
    onquick,
    searchInput = $bindable(),
  }: {
    hosts: Host[];
    connected: Set<string>;
    onconnect: (h: Host) => void;
    onedit: (h: Host) => void;
    onnew: (group: string | null) => void;
    /** 快速连接：`ssh user@host -p 22` / `user@host:port` */
    onquick: (target: { username: string; address: string; port: number }) => void;
    searchInput?: HTMLInputElement;
  } = $props();

  let query = $state('');
  /** 当前打开的分组（null = 全部） */
  let group = $state<string | null>(null);

  /** 解析快速连接输入 */
  const quick = $derived.by(() => {
    let q = query.trim();
    if (!q) return null;
    let port = 22;
    q = q.replace(/^ssh\s+/, '');
    const p = q.match(/\s-p\s*(\d+)/);
    if (p) {
      port = +p[1];
      q = q.replace(p[0], '').trim();
    }
    const m = q.match(/^([^@\s]+)@([^\s:]+)(?::(\d+))?$/);
    if (!m) return null;
    return { username: m[1], address: m[2], port: m[3] ? +m[3] : port };
  });

  const groups = $derived.by(() => {
    const map = new Map<string, number>();
    for (const h of hosts) if (h.group?.trim()) map.set(h.group.trim(), (map.get(h.group.trim()) ?? 0) + 1);
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  });

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let list = hosts;
    if (q && !quick) {
      list = list.filter((h) => [h.name, h.address, h.username, h.group ?? '', h.os ?? ''].some((s) => s.toLowerCase().includes(q)));
    } else if (group) {
      list = list.filter((h) => h.group?.trim() === group);
    }
    return [...list].sort((a, b) => a.name.localeCompare(b.name));
  });

  /** 卡片图标：远端系统的 logo；RDP / VNC 用显示器，串口 / Telnet 用终端，未识别用服务器图标 */
  function tileFor(h: Host) {
    if (h.protocol === 'rdp' || h.protocol === 'vnc') return { bg: '#2563EB', fg: '#fff', name: h.protocol.toUpperCase(), icon: 'monitor' as const, path: null };
    if (h.protocol === 'serial' || h.protocol === 'telnet') return { bg: '#57534E', fg: '#fff', name: h.protocol, icon: 'terminal' as const, path: null };
    const os = osIcon(h.os);
    if (os) return { bg: os.bg, fg: os.fg, name: os.name, icon: null, path: os.path };
    return { bg: 'color-mix(in srgb, var(--acc) 80%, #000)', fg: '#fff', name: h.os ? h.os : '连接后自动识别系统', icon: 'host' as const, path: null };
  }

  function submitSearch(e: Event) {
    e.preventDefault();
    if (quick) {
      onquick(quick);
      query = '';
    } else if (shown.length === 1) {
      onconnect(shown[0]);
    }
  }
</script>

<section class="hv" aria-label="主机列表" in:fade={{ duration: 160 }}>
  <form class="bar" onsubmit={submitSearch}>
    <Icon name="search" size={15} />
    <input bind:this={searchInput} bind:value={query} placeholder="查找主机，或输入 ssh user@hostname 直接连接" aria-label="搜索主机" spellcheck="false" autocomplete="off" />
    {#if quick}
      <button class="btn" type="submit" in:fade={{ duration: 120 }}>连接 {quick.username}@{quick.address}</button>
    {:else}
      <kbd>⌘K</kbd>
    {/if}
  </form>

  <div class="tools">
    {#if hosts.length}
      <button class="btn" type="button" onclick={() => onnew(group)}><Icon name="plus" size={14} stroke={2} />新建主机</button>
    {/if}
    {#if group}
      <nav class="crumb" aria-label="当前位置" in:fade={{ duration: 120 }}>
        <button type="button" class="link" onclick={() => (group = null)}>全部主机</button>
        <span class="flip"><Icon name="chevron" size={13} /></span>
        <b>{group}</b>
      </nav>
    {/if}
  </div>

  <div class="scroll">
    {#if !hosts.length}
      <div class="empty">
        <Logo size={56} blink />
        <h1>欢迎使用 Vornel</h1>
        <p>添加第一台主机，开始连接。也可以在“设置 → 安全 → 导入数据”从其他客户端迁移。</p>
        <button class="btn" type="button" onclick={() => onnew(null)}><Icon name="plus" size={14} stroke={2} />新建主机</button>
      </div>
    {:else}
      {#if !group && !query.trim() && groups.length}
        <h2>分组</h2>
        <div class="grid">
          {#each groups as [g, n], i (g)}
            <button class="card" type="button" style:--i={i} onclick={() => (group = g)} title="打开分组 {g}">
              <span class="tile grp"><Icon name="group" size={20} /></span>
              <span class="txt"><b>{g}</b><small>{n} 台主机</small></span>
            </button>
          {/each}
        </div>
      {/if}

      <h2>{group ?? (query.trim() && !quick ? `搜索结果（${shown.length}）` : '主机')}</h2>
      {#if shown.length}
        <div class="grid">
          {#each shown as h, i (h.id)}
            {@const st = tileFor(h)}
            <div class="cell" animate:flip={{ duration: 180 }}>
              <button
                class="card"
                type="button"
                style:--i={Math.min(i, 24)}
                onclick={() => onconnect(h)}
                oncontextmenu={(e) => {
                  e.preventDefault();
                  onedit(h);
                }}
                title="连接 {h.username ? `${h.username}@` : ''}{h.address}:{h.port}（右键编辑）"
              >
                <span class="tile" style:background={st.bg} style:color={st.fg} title={st.name}>
                  {#if st.path}
                    <svg class="logo" viewBox="0 0 24 24" aria-hidden="true"><path d={st.path} fill="currentColor" /></svg>
                  {:else if st.icon}
                    <Icon name={st.icon} size={19} />
                  {/if}
                  {#if connected.has(h.id)}<span class="live" aria-label="已连接"></span>{/if}
                </span>
                <span class="txt">
                  <b>{h.name}</b>
                  <small class="mono">{h.protocol}{h.username ? `, ${h.username}` : ''}</small>
                </span>
              </button>
              <button class="edit icon-btn" type="button" aria-label="编辑 {h.name}" onclick={() => onedit(h)}><Icon name="edit" size={14} /></button>
            </div>
          {/each}
        </div>
      {:else}
        <p class="none">{quick ? '按回车直接连接，连接信息会保存为一台新主机。' : `没有匹配“${query}”的主机`}</p>
      {/if}
    {/if}
  </div>
</section>

<style>
  .hv {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--win);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 14px 22px 0;
    height: 42px;
    padding: 0 6px 0 14px;
    border-radius: 10px;
    background: var(--raise);
    box-shadow: 0 0 0 1px var(--line2);
    color: var(--mut);
    transition: box-shadow 160ms ease-out;
  }
  .bar:focus-within {
    box-shadow: 0 0 0 2px var(--acc);
  }
  .bar input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: 0;
    background: none;
    color: var(--fg);
    font: 14px var(--f-ui);
  }
  kbd {
    font: 500 11px var(--f-mono);
    padding: 1px 6px;
    margin-right: 6px;
    border-radius: 4px;
    border: 1px solid var(--line2);
  }
  .bar .btn {
    height: 30px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 22px 4px;
  }
  .crumb {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--mut);
  }
  .crumb b {
    color: var(--fg);
    font-weight: 600;
  }
  .flip {
    display: inline-flex;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 22px 28px;
  }
  h2 {
    margin: 18px 0 10px;
    font-size: 13px;
    font-weight: 600;
    color: var(--mut);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
  }
  .cell {
    position: relative;
  }
  .card {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 62px;
    padding: 0 12px;
    border: 0;
    border-radius: 11px;
    background: var(--raise);
    box-shadow: 0 0 0 1px var(--line);
    color: var(--fg);
    text-align: left;
    cursor: pointer;
    animation: rise 260ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
    animation-delay: calc(var(--i, 0) * 14ms);
    transition:
      transform 140ms ease-out,
      box-shadow 140ms ease-out,
      background 140ms ease-out;
  }
  .card:hover {
    transform: translateY(-2px);
    box-shadow:
      0 0 0 1px var(--line2),
      0 8px 20px -10px rgba(0, 0, 0, 0.45);
  }
  .card:active {
    transform: translateY(0) scale(0.985);
    transition-duration: 60ms;
  }
  .tile {
    position: relative;
    width: 38px;
    height: 38px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 9px;
    color: #fff;
    transition: transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .card:hover .tile {
    transform: scale(1.06) rotate(-3deg);
  }
  .tile.grp {
    background: color-mix(in srgb, var(--acc) 85%, #000);
  }
  .logo {
    width: 21px;
    height: 21px;
  }
  .live {
    position: absolute;
    right: -3px;
    bottom: -3px;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 0 2px var(--raise);
    animation: pulse 2s ease-out infinite;
  }
  .txt {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .txt b {
    font-weight: 500;
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .txt small {
    font-size: 11.5px;
    color: var(--mut);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .edit {
    position: absolute;
    right: 8px;
    top: 50%;
    translate: 0 -50%;
    opacity: 0;
    transition: opacity 120ms ease-out;
  }
  .cell:hover .edit,
  .edit:focus-visible {
    opacity: 1;
  }
  .none {
    color: var(--mut);
    font-size: 13px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding-top: 12vh;
    text-align: center;
  }
  .empty h1 {
    margin: 8px 0 0;
    font-size: 22px;
  }
  .empty p {
    margin: 0 0 8px;
    color: var(--mut);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  @keyframes pulse {
    0% {
      box-shadow:
        0 0 0 2px var(--raise),
        0 0 0 2px color-mix(in srgb, var(--ok) 60%, transparent);
    }
    100% {
      box-shadow:
        0 0 0 2px var(--raise),
        0 0 0 9px transparent;
    }
  }
</style>
