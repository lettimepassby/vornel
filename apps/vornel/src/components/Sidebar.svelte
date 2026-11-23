<script lang="ts">
  import type { Host } from '../lib/api';
  import Icon from './Icon.svelte';
  import Logo from './Logo.svelte';

  let {
    hosts,
    connected,
    macInset,
    onconnect,
    onedit,
    onnew,
    onsettings,
    searchInput = $bindable(),
  }: {
    hosts: Host[];
    /** 已连接主机的 id */
    connected: Set<string>;
    macInset: boolean;
    onconnect: (h: Host) => void;
    onedit: (h: Host) => void;
    onnew: () => void;
    onsettings: () => void;
    searchInput?: HTMLInputElement;
  } = $props();

  let query = $state('');

  const groups = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const filtered = q
      ? hosts.filter((h) => [h.name, h.address, h.username, h.group ?? ''].some((s) => s.toLowerCase().includes(q)))
      : hosts;
    const map = new Map<string, Host[]>();
    for (const h of filtered) {
      const g = h.group?.trim() || '未分组';
      if (!map.has(g)) map.set(g, []);
      map.get(g)!.push(h);
    }
    return [...map.entries()].sort(([a], [b]) => (a === '未分组' ? 1 : b === '未分组' ? -1 : a.localeCompare(b)));
  });
</script>

<aside class="sb">
  <div class="top" class:mac={macInset} data-tauri-drag-region>
    {#if !macInset}<Logo size={18} wordmark />{/if}
    <span class="sp" data-tauri-drag-region></span>
    <button class="icon-btn" type="button" aria-label="新建主机" title="新建主机" onclick={onnew}>
      <Icon name="plus" />
    </button>
  </div>

  <label class="search">
    <Icon name="search" size={14} />
    <input bind:this={searchInput} bind:value={query} placeholder="搜索主机" aria-label="搜索主机" />
    <kbd>⌘K</kbd>
  </label>

  <nav class="nav" aria-label="主导航">
    <button class="ni on" type="button" aria-current="page"><Icon name="host" size={15} />主机<span class="c">{hosts.length}</span></button>
    <button class="ni" type="button" disabled title="M2 开发中"><Icon name="key" size={15} />密钥串</button>
    <button class="ni" type="button" disabled title="M2 开发中"><Icon name="code" size={15} />代码片段</button>
    <button class="ni" type="button" disabled title="M2 开发中"><Icon name="forward" size={15} />端口转发</button>
  </nav>

  <div class="hosts">
    {#each groups as [group, list] (group)}
      <div class="sec"><span>{group}</span><span class="mono">{list.length}</span></div>
      {#each list as h (h.id)}
        <div class="row">
          <button class="h" type="button" onclick={() => onconnect(h)} title="连接 {h.username}@{h.address}">
            <span class="dot" class:ok={connected.has(h.id)}></span>
            <span class="nm">{h.name}</span>
            <span class="pt">{h.protocol}</span>
          </button>
          <button class="edit icon-btn" type="button" aria-label="编辑 {h.name}" onclick={() => onedit(h)}>
            <Icon name="edit" size={14} />
          </button>
        </div>
      {/each}
    {:else}
      <div class="empty">
        {#if query}没有匹配“{query}”的主机{:else}还没有主机<button class="link" type="button" onclick={onnew}>新建一台</button>{/if}
      </div>
    {/each}
  </div>

  <div class="foot">
    <Icon name="cloud" size={16} />
    <div class="sync"><b>同步未开启</b><span>M3 开发中</span></div>
    <button class="icon-btn" type="button" aria-label="设置" title="设置 ⌘," onclick={onsettings}>
      <Icon name="settings" />
    </button>
  </div>
</aside>

<style>
  .sb {
    width: 236px;
    flex: none;
    display: flex;
    flex-direction: column;
    padding: 0 10px 10px;
    background: var(--sb);
    border-right: 1px solid var(--line);
  }
  .top {
    height: 46px;
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 0 0 6px;
  }
  .top.mac {
    padding-left: 78px;
  }
  .sp {
    flex: 1;
    align-self: stretch;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 9px;
    margin-bottom: 12px;
    border-radius: 7px;
    border: 1px solid var(--line);
    background: var(--raise);
    color: var(--mut);
  }
  .search:focus-within {
    border-color: var(--acc);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    font-size: 12.5px;
  }
  .search input::placeholder {
    color: var(--mut);
  }
  kbd {
    font: 500 11px var(--f-mono);
    color: var(--mut);
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .ni {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 29px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--mut);
    text-align: left;
  }
  .ni:disabled {
    cursor: default;
    opacity: 0.55;
  }
  .ni.on {
    background: var(--sel);
    color: var(--fg);
  }
  .c {
    margin-left: auto;
    font: 11px var(--f-mono);
    color: var(--mut);
  }
  .hosts {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 0 -4px;
    padding: 0 4px;
  }
  .sec {
    display: flex;
    justify-content: space-between;
    margin: 18px 8px 4px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--mut);
  }
  .sec .mono {
    font-size: 11px;
  }
  .row {
    position: relative;
  }
  .h {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    transition: background 120ms ease-out;
  }
  .h:hover {
    background: var(--sel);
  }
  .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: 12.5px var(--f-mono);
  }
  .pt {
    font: 11px var(--f-mono);
    color: var(--mut);
  }
  .edit {
    position: absolute;
    right: 2px;
    top: 2px;
    width: 26px;
    height: 26px;
    opacity: 0;
    background: var(--sb);
  }
  .row:hover .edit,
  .edit:focus-visible {
    opacity: 1;
  }
  .row:hover .pt {
    visibility: hidden;
  }
  .empty {
    margin: 24px 8px;
    color: var(--mut);
    font-size: 12.5px;
    line-height: 1.7;
  }
  .link {
    border: 0;
    background: none;
    padding: 0 0 0 6px;
    color: var(--acc);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 2px 0 6px;
    border-top: 1px solid var(--line);
    color: var(--mut);
  }
  .sync {
    flex: 1;
    display: flex;
    flex-direction: column;
    font-size: 12px;
  }
  .sync b {
    color: var(--fg);
    font-weight: 500;
    font-size: 12.5px;
  }
</style>
