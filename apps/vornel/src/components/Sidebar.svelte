<script lang="ts" module>
  export type View = 'hosts' | 'keys' | 'snippets' | 'forwards' | 'known';
</script>

<script lang="ts">
  import Icon from './Icon.svelte';
  import Logo from './Logo.svelte';

  let {
    macInset,
    view,
    counts,
    sync,
    onview,
    onnew,
    onsettings,
    onsync,
  }: {
    macInset: boolean;
    view: View;
    counts: Record<View, number>;
    sync: { title: string; detail: string; state: 'off' | 'ok' | 'busy' | 'err' };
    onview: (v: View) => void;
    onnew: () => void;
    onsettings: () => void;
    onsync: () => void;
  } = $props();

  const NAV: { id: View; label: string; icon: 'host' | 'key' | 'code' | 'forward' | 'shield' }[] = [
    { id: 'hosts', label: '主机', icon: 'host' },
    { id: 'keys', label: '密钥串', icon: 'key' },
    { id: 'snippets', label: '代码片段', icon: 'code' },
    { id: 'forwards', label: '端口转发', icon: 'forward' },
    { id: 'known', label: '已知主机', icon: 'shield' },
  ];

</script>

<aside class="sb">
  <div class="top" class:mac={macInset} data-tauri-drag-region>
    {#if !macInset}<Logo size={18} wordmark />{/if}
    <span class="sp" data-tauri-drag-region></span>
    <button class="icon-btn" type="button" aria-label="新建主机" title="新建主机 ⌘N" onclick={onnew}>
      <Icon name="plus" />
    </button>
  </div>


  <nav class="nav" aria-label="主导航">
    {#each NAV as n (n.id)}
      <button class="ni" class:on={view === n.id} type="button" aria-current={view === n.id ? 'page' : undefined} onclick={() => onview(n.id)}>
        <Icon name={n.icon} size={15} />{n.label}<span class="c">{counts[n.id]}</span>
      </button>
    {/each}
  </nav>

  <div class="grow"></div>

  <div class="foot">
    <button class="sync-btn st-{sync.state}" type="button" onclick={onsync} title="立即同步">
      <Icon name={sync.state === 'busy' ? 'refresh' : 'cloud'} size={16} />
      <span class="sync"><b>{sync.title}</b><span>{sync.detail}</span></span>
    </button>
    <button class="icon-btn" type="button" aria-label="设置" title="设置 ⌘," onclick={onsettings}>
      <Icon name="settings" />
    </button>
  </div>
</aside>

<style>
  .grow {
    flex: 1;
  }
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
    padding-left: 84px;
  }
  .sp {
    flex: 1;
    align-self: stretch;
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
  .ni.on {
    background: var(--sel);
    color: var(--fg);
  }
  .c {
    margin-left: auto;
    font: 11px var(--f-mono);
    color: var(--mut);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 2px 0 6px;
    border-top: 1px solid var(--line);
    color: var(--mut);
  }
  .sync-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--mut);
    text-align: left;
  }
  .sync-btn:hover {
    background: var(--sel);
  }
  .st-ok {
    color: var(--ok);
  }
  .st-err {
    color: var(--err);
  }
  .st-busy :global(svg) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .sync {
    flex: 1;
    min-width: 0;
    color: var(--mut);
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
