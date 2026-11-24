<script lang="ts">
  import { flip } from 'svelte/animate';
  import { fade } from 'svelte/transition';
  import type { Host } from '../lib/api';
  import Icon from '../components/Icon.svelte';
  import Logo from '../components/Logo.svelte';
  import { osIcon } from '../lib/os-icons';
  import { prefs } from '../lib/store.svelte';
  import GroupDialog from '../components/GroupDialog.svelte';
  import GroupTile from '../components/GroupTile.svelte';
  import { openMenu } from '../lib/contextmenu.svelte';
  import { copyText } from '../lib/clipboard';

  let {
    hosts,
    connected,
    onconnect,
    onedit,
    onduplicate,
    ondelete,
    onnew,
    onquick,
    searchInput = $bindable(),
  }: {
    hosts: Host[];
    connected: Set<string>;
    onconnect: (h: Host) => void;
    onedit: (h: Host) => void;
    /** 以这台主机为模板新建（复制一份） */
    onduplicate: (h: Host) => void;
    ondelete: (h: Host) => void;
    onnew: (group: string | null) => void;
    /** 快速连接：`ssh user@host -p 22` / `user@host:port` */
    onquick: (target: { username: string; address: string; port: number }) => void;
    searchInput?: HTMLInputElement;
  } = $props();

  const SEARCH_KEY = /Mac|iPhone|iPad/.test(navigator.userAgent) ? '⌘K' : 'Ctrl+K';
  let query = $state('');
  /** 当前打开的分组（null = 全部） */
  let group = $state<string | null>(null);
  /** 正在编辑外观的分组 */
  let editingGroup = $state<string | null>(null);

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
    // 最近连接过的排在前面，没连过的按名称排序
    return [...list].sort((a, b) => (b.lastConnected ?? 0) - (a.lastConnected ?? 0) || a.name.localeCompare(b.name));
  });

  /** 卡片图标：远端系统的 logo；RDP / VNC 用显示器，串口 / Telnet 用终端，未识别用服务器图标 */
  function tileFor(h: Host) {
    if (h.protocol === 'rdp' || h.protocol === 'vnc') return { bg: '#2563EB', fg: '#fff', name: h.protocol.toUpperCase(), icon: 'monitor' as const, path: null };
    if (h.protocol === 'serial' || h.protocol === 'telnet') return { bg: '#57534E', fg: '#fff', name: h.protocol, icon: 'terminal' as const, path: null };
    const os = osIcon(h.os);
    if (os) return { bg: os.bg, fg: os.fg, name: os.name, icon: null, path: os.path };
    return { bg: 'color-mix(in srgb, var(--acc) 80%, #000)', fg: '#fff', name: h.os ? h.os : '连接后自动识别系统', icon: 'host' as const, path: null };
  }

  // ---- 右键菜单
  function hostMenu(e: MouseEvent, h: Host) {
    const target = `${h.username ? `${h.username}@` : ''}${h.address}`;
    openMenu(e, [
      { label: '连接', icon: 'play', action: () => onconnect(h) },
      'sep',
      { label: '编辑…', icon: 'edit', action: () => onedit(h) },
      { label: '复制一份', icon: 'copy', action: () => onduplicate(h) },
      'sep',
      { label: '复制地址', action: () => copyText(h.address) },
      ...(h.protocol === 'ssh' ? [{ label: '复制 SSH 命令', icon: 'terminal' as const, action: () => copyText(`ssh ${h.port !== 22 ? `-p ${h.port} ` : ''}${target}`) }] : []),
      'sep',
      { label: '删除…', icon: 'trash', danger: true, action: () => ondelete(h) },
    ]);
  }

  function groupMenu(e: MouseEvent, g: string) {
    openMenu(e, [
      { label: '打开分组', icon: 'group', action: () => (group = g) },
      { label: '在此分组新建主机', icon: 'plus', action: () => onnew(g) },
      'sep',
      { label: '分组外观…', icon: 'edit', action: () => (editingGroup = g) },
    ]);
  }

  function blankMenu(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.card, input, button')) return;
    openMenu(e, [{ label: group ? `在「${group}」新建主机` : '新建主机', icon: 'plus', action: () => onnew(group) }]);
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
      <kbd>{SEARCH_KEY}</kbd>
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

  <div class="scroll" role="presentation" oncontextmenu={blankMenu}>
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
            <div class="cell">
              <button
                class="card"
                type="button"
                style:--i={i}
                onclick={() => (group = g)}
                oncontextmenu={(e) => groupMenu(e, g)}
                title="打开分组 {g}"
              >
                <span class="gt"><GroupTile style={prefs.groupStyles?.[g]} /></span>
                <span class="txt"><b>{g}</b><small>{n} 台主机</small></span>
              </button>
              <div class="acts">
                <button class="icon-btn" type="button" aria-label="设置分组 {g} 的外观" title="分组外观" onclick={() => (editingGroup = g)}><Icon name="edit" size={14} /></button>
              </div>
            </div>
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
                oncontextmenu={(e) => hostMenu(e, h)}
                title="连接 {h.username ? `${h.username}@` : ''}{h.address}:{h.port}（右键查看更多操作）"
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
              <div class="acts">
                <button class="icon-btn" type="button" aria-label="复制 {h.name}" title="复制一份" onclick={() => onduplicate(h)}><Icon name="copy" size={14} /></button>
                <button class="icon-btn" type="button" aria-label="编辑 {h.name}" title="编辑" onclick={() => onedit(h)}><Icon name="edit" size={14} /></button>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <p class="none">{quick ? '按回车直接连接，连接信息会保存为一台新主机。' : `没有匹配“${query}”的主机`}</p>
      {/if}
    {/if}
  </div>
</section>

{#if editingGroup}
  <GroupDialog
    group={editingGroup}
    onclose={() => (editingGroup = null)}
    onrenamed={(n) => {
      if (group === editingGroup) group = n;
    }}
  />
{/if}

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
    /* 260px：常见窗口宽度下一行 4 张，主机名约 21 个字符内能完整显示 */
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
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
  .gt {
    display: flex;
    transition: transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .card:hover .gt {
    transform: scale(1.06) rotate(-3deg);
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
  /* 悬停时才浮出的操作按钮：平时不占位置，名字可以用满整张卡片 */
  .acts {
    position: absolute;
    top: 1px;
    right: 1px;
    bottom: 1px;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px 0 28px;
    border-radius: 0 10px 10px 0;
    background: linear-gradient(to right, transparent, var(--raise) 24px);
    opacity: 0;
    translate: 6px 0;
    pointer-events: none;
    transition:
      opacity 140ms ease-out,
      translate 140ms ease-out;
  }
  .cell:hover .acts,
  .acts:focus-within {
    opacity: 1;
    translate: 0 0;
    pointer-events: auto;
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
