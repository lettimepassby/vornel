<script lang="ts">
  // 分屏：选择与当前会话一起显示的会话或主机（已打开的会话直接加入，主机会新开会话），最多 4 屏
  import { untrack } from 'svelte';
  import type { Host } from '../lib/api';
  import { osIcon } from '../lib/os-icons';
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';

  type Layout = 'cols' | 'rows' | 'grid';
  interface SessionItem {
    key: string;
    title: string;
    host: Host;
    state: string;
  }

  let {
    current,
    sessions,
    hosts,
    initial,
    inSplit,
    onconfirm,
    onexit,
    onclose,
  }: {
    /** 当前会话的标题 */
    current: string;
    /** 其他已打开的终端会话 */
    sessions: SessionItem[];
    hosts: Host[];
    initial: { keys: string[]; layout: Layout; broadcast: boolean } | null;
    inSplit: boolean;
    onconfirm: (r: { tabKeys: string[]; hosts: Host[]; layout: Layout; broadcast: boolean }) => void;
    onexit: () => void;
    onclose: () => void;
  } = $props();

  const MAX = 3;
  let pickedTabs = $state<string[]>(untrack(() => (initial ? initial.keys.filter((k) => sessions.some((s) => s.key === k)) : [])));
  let pickedHosts = $state<string[]>([]);
  let pairLayout = $state<'cols' | 'rows'>(untrack(() => (initial?.layout === 'rows' ? 'rows' : 'cols')));
  let broadcast = $state(untrack(() => initial?.broadcast ?? false));
  let query = $state('');

  const count = $derived(pickedTabs.length + pickedHosts.length);
  const full = $derived(count >= MAX);
  const shownHosts = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = q ? hosts.filter((h) => [h.name, h.address, h.username, h.group ?? ''].some((s) => s.toLowerCase().includes(q))) : hosts;
    return list.filter((h) => h.protocol === 'ssh' || h.protocol === 'mosh' || h.protocol === 'telnet' || h.protocol === 'serial');
  });

  function toggle(list: string[], id: string) {
    return list.includes(id) ? list.filter((x) => x !== id) : full ? list : [...list, id];
  }

  function confirm() {
    onconfirm({
      tabKeys: pickedTabs,
      hosts: pickedHosts.map((id) => hosts.find((h) => h.id === id)!).filter(Boolean),
      layout: count === 1 ? pairLayout : 'grid',
      broadcast,
    });
  }
</script>

<Dialog title="分屏" width={520} {onclose}>
  <div class="body">
    <p class="lead">
      与 <b class="mono">{current}</b> 一起显示。最多再选 {MAX} 个（已选 <b class="mono">{count}</b>）。
    </p>

    {#if sessions.length}
      <h3>已打开的会话</h3>
      <div class="list" role="group" aria-label="已打开的会话">
        {#each sessions as s (s.key)}
          {@const on = pickedTabs.includes(s.key)}
          <label class="row" class:on class:off={!on && full}>
            <input type="checkbox" checked={on} disabled={!on && full} onchange={() => (pickedTabs = toggle(pickedTabs, s.key))} />
            <span class="dot {s.state === 'connected' ? 'ok' : s.state === 'connecting' ? 'busy' : ''}"></span>
            <span class="nm">{s.title}</span>
            <span class="sub mono">{s.host.username ? `${s.host.username}@` : ''}{s.host.address}</span>
          </label>
        {/each}
      </div>
    {/if}

    <h3>主机（新开会话）</h3>
    <label class="search">
      <Icon name="search" size={14} />
      <input bind:value={query} placeholder="搜索主机" aria-label="搜索要分屏的主机" />
    </label>
    <div class="list hosts" role="group" aria-label="主机">
      {#each shownHosts as h (h.id)}
        {@const on = pickedHosts.includes(h.id)}
        {@const os = osIcon(h.os)}
        <label class="row" class:on class:off={!on && full}>
          <input type="checkbox" checked={on} disabled={!on && full} onchange={() => (pickedHosts = toggle(pickedHosts, h.id))} />
          <span class="tile" style:background={os?.bg ?? 'color-mix(in srgb, var(--acc) 80%, #000)'} style:color={os?.fg ?? '#fff'}>
            {#if os}<svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path d={os.path} fill="currentColor" /></svg>{:else}<Icon name="host" size={11} />{/if}
          </span>
          <span class="nm">{h.name}</span>
          <span class="sub mono">{h.username ? `${h.username}@` : ''}{h.address}</span>
        </label>
      {:else}
        <p class="none">没有匹配的主机</p>
      {/each}
    </div>

    <div class="opts">
      {#if count === 1}
        <div class="seg" role="group" aria-label="分屏方向">
          <button type="button" aria-pressed={pairLayout === 'cols'} onclick={() => (pairLayout = 'cols')}>左右</button>
          <button type="button" aria-pressed={pairLayout === 'rows'} onclick={() => (pairLayout = 'rows')}>上下</button>
        </div>
      {:else if count > 1}
        <span class="hint">{count + 1} 个会话以四宫格显示</span>
      {/if}
      <label class="check"><input type="checkbox" bind:checked={broadcast} />同步输入（键盘输入发送到所有分屏）</label>
    </div>

    <div class="dialog-actions">
      {#if inSplit}<button class="btn danger" type="button" onclick={onexit}>退出分屏</button>{/if}
      <span class="grow"></span>
      <button class="btn ghost" type="button" onclick={onclose}>取消</button>
      <button class="btn" type="button" disabled={count === 0} onclick={confirm}><Icon name="split" size={14} />开始分屏</button>
    </div>
  </div>
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .lead {
    margin: 0;
    font-size: 13px;
    color: var(--mut);
  }
  .lead b {
    color: var(--fg);
  }
  h3 {
    margin: 8px 0 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--mut);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 168px;
    overflow-y: auto;
  }
  .list.hosts {
    max-height: 220px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 32px;
    padding: 0 8px;
    border-radius: 7px;
    cursor: pointer;
    transition: background 120ms ease-out;
  }
  .row:hover {
    background: var(--sel);
  }
  .row.on {
    background: var(--accbg);
  }
  .row.off {
    opacity: 0.45;
    cursor: default;
  }
  .row input {
    accent-color: var(--acc);
  }
  .nm {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    margin-left: auto;
    font-size: 11px;
    color: var(--mut);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 45%;
  }
  .tile {
    width: 20px;
    height: 20px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 5px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px;
    border-radius: 7px;
    background: var(--raise);
    box-shadow: 0 0 0 1px var(--line2);
    color: var(--mut);
  }
  .search input {
    flex: 1;
    border: 0;
    outline: 0;
    background: none;
    color: var(--fg);
    font: 13px var(--f-ui);
  }
  .none {
    margin: 6px 8px;
    font-size: 12.5px;
    color: var(--mut);
  }
  .opts {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 14px;
    margin-top: 6px;
  }
  .hint {
    font-size: 12.5px;
    color: var(--mut);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
  }
  .grow {
    flex: 1;
  }
</style>
