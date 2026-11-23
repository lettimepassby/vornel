<script lang="ts">
  // iOS 主机页（DESIGN.md 5.4）：大标题、搜索、圆角分组列表。
  import { PROTOCOL_LABELS, type Host } from '../lib/api';
  import Icon from './Icon.svelte';

  let {
    hosts,
    sessions,
    onconnect,
    onresume,
    onedit,
    onnew,
    onknown,
  }: {
    hosts: Host[];
    /** 已打开的会话 */
    sessions: { key: string; title: string; state: string }[];
    onconnect: (h: Host) => void;
    onresume: (key: string) => void;
    onedit: (h: Host) => void;
    onnew: () => void;
    onknown: () => void;
  } = $props();

  let query = $state('');

  const groups = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = q ? hosts.filter((h) => [h.name, h.address, h.username].some((s) => s.toLowerCase().includes(q))) : hosts;
    const map = new Map<string, Host[]>();
    for (const h of list) {
      const g = h.group?.trim() || '未分组';
      if (!map.has(g)) map.set(g, []);
      map.get(g)!.push(h);
    }
    return [...map.entries()];
  });

  const badge = (h: Host) => (h.protocol === 'ssh' ? h.address.slice(0, 2).toUpperCase() : PROTOCOL_LABELS[h.protocol].slice(0, 3));
</script>

<div class="m">
  <div class="head">
    <h1>主机</h1>
    <button class="circ" type="button" aria-label="新建主机" onclick={onnew}><Icon name="plus" size={18} stroke={2} /></button>
  </div>
  <label class="search">
    <Icon name="search" size={16} />
    <input bind:value={query} placeholder="搜索" aria-label="搜索主机" />
  </label>

  {#if sessions.length}
    <div class="grp">活动会话</div>
    <div class="list">
      {#each sessions as s (s.key)}
        <button class="row" type="button" onclick={() => onresume(s.key)}>
          <span class="os"><Icon name="terminal" size={16} /></span>
          <span class="t"><b>{s.title}</b><small>{s.state === 'connected' ? '已连接' : s.state === 'connecting' ? '正在连接' : '已断开'}</small></span>
          <span class="dot" class:ok={s.state === 'connected'}></span>
          <span class="chev"><Icon name="chevron" size={14} /></span>
        </button>
      {/each}
    </div>
  {/if}

  {#each groups as [group, list] (group)}
    <div class="grp">{group}</div>
    <div class="list">
      {#each list as h (h.id)}
        <div class="row-wrap">
          <button class="row" type="button" onclick={() => onconnect(h)}>
            <span class="os">{badge(h)}</span>
            <span class="t">
              <b>{h.name}</b>
              <small>{h.protocol !== 'ssh' ? PROTOCOL_LABELS[h.protocol] + ' · ' : ''}{h.username ? h.username + '@' : ''}{h.address}</small>
            </span>
          </button>
          <button class="edit" type="button" aria-label="编辑 {h.name}" onclick={() => onedit(h)}><Icon name="edit" size={16} /></button>
        </div>
      {/each}
    </div>
  {:else}
    <p class="empty">{query ? '没有匹配的主机' : '还没有主机，点右上角的 + 添加。'}</p>
  {/each}

  <button class="link" type="button" onclick={onknown}>已知主机</button>
</div>

<style>
  .m {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    padding: 4px 0 24px;
    background: var(--win);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 18px 0;
  }
  h1 {
    margin: 0;
    font-size: 32px;
    font-weight: 700;
    letter-spacing: -0.03em;
  }
  .circ {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: 0;
    background: var(--accbg);
    color: var(--acc);
    display: grid;
    place-items: center;
  }
  .search {
    margin: 12px 16px 0;
    height: 38px;
    border-radius: 11px;
    background: var(--raise);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    color: var(--mut);
  }
  .search input {
    flex: 1;
    border: 0;
    outline: none;
    background: none;
    font-size: 16px;
  }
  .grp {
    margin: 22px 30px 8px;
    font-size: 13px;
    font-weight: 500;
    color: var(--mut);
  }
  .list {
    margin: 0 16px;
    background: var(--raise);
    border-radius: 14px;
    overflow: hidden;
  }
  .row-wrap {
    display: flex;
    align-items: center;
    position: relative;
  }
  .row-wrap + .row-wrap::before,
  .row + .row::before {
    content: '';
    position: absolute;
    top: 0;
    left: 60px;
    right: 0;
    border-top: 1px solid var(--line);
  }
  .row {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 60px;
    padding: 0 14px;
    border: 0;
    background: none;
    text-align: left;
  }
  .os {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: var(--sel);
    display: grid;
    place-items: center;
    font: 600 11px var(--f-mono);
    color: var(--mut);
    flex: none;
  }
  .t {
    flex: 1;
    min-width: 0;
  }
  .t b {
    display: block;
    font: 500 15px var(--f-mono);
  }
  .t small {
    display: block;
    font-size: 12.5px;
    color: var(--mut);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    color: var(--dim);
  }
  .edit {
    width: 44px;
    height: 44px;
    border: 0;
    background: none;
    color: var(--mut);
    display: grid;
    place-items: center;
  }
  .empty {
    margin: 40px 24px;
    text-align: center;
    color: var(--mut);
  }
  .link {
    display: block;
    margin: 24px auto 0;
    border: 0;
    background: none;
    color: var(--acc);
    font-size: 15px;
    min-height: 44px;
  }
</style>
