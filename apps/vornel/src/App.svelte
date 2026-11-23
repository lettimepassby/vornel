<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import * as api from './lib/api';
  import type { Host, Snippet } from './lib/api';
  import { listAddons } from './lib/addons';
  import { applyFontPack } from './lib/fonts';
  import { data, loadAll, persistPrefs, prefs } from './lib/store.svelte';
  import HostDialog from './components/HostDialog.svelte';
  import Icon from './components/Icon.svelte';
  import Logo from './components/Logo.svelte';
  import RemoteDesktopPane from './components/RemoteDesktopPane.svelte';
  import SettingsDialog from './components/SettingsDialog.svelte';
  import SftpPanel from './components/SftpPanel.svelte';
  import Sidebar, { type View } from './components/Sidebar.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import TabBar from './components/TabBar.svelte';
  import TerminalPane, { type PaneStatus } from './components/TerminalPane.svelte';
  import ForwardsView from './views/ForwardsView.svelte';
  import KeysView from './views/KeysView.svelte';
  import KnownHostsView from './views/KnownHostsView.svelte';
  import SnippetsView from './views/SnippetsView.svelte';

  interface Tab {
    key: string;
    host: Host;
    status: PaneStatus;
    sftp: boolean;
  }

  // macOS 使用 Overlay 标题栏，红绿灯落在侧边栏左上角，需要留出位置
  const macInset = /Macintosh|Mac OS X/.test(navigator.userAgent) && !/iPhone|iPad/.test(navigator.userAgent);
  const isTerminal = (h: Host) => ['ssh', 'mosh', 'telnet', 'serial'].includes(h.protocol);

  let view = $state<View>('hosts');
  let tabs = $state<Tab[]>([]);
  let activeKey = $state<string | null>(null);
  let editing = $state<{ host: Host; isNew: boolean } | null>(null);
  let settingsTab = $state<'appearance' | 'sync' | null>(null);
  let searchInput = $state<HTMLInputElement>();
  let snippetMenu = $state(false);
  const panes: Record<string, { send: (text: string) => void } | undefined> = $state({});

  let sync = $state<{ title: string; detail: string; state: 'off' | 'ok' | 'busy' | 'err' }>({
    title: '同步未开启',
    detail: '点击设置',
    state: 'off',
  });

  const activeTab = $derived(tabs.find((t) => t.key === activeKey) ?? null);
  const connectedIds = $derived(new Set(tabs.filter((t) => t.status.state === 'connected').map((t) => t.host.id)));
  const counts = $derived({
    hosts: data.hosts.length,
    keys: data.keys.length,
    snippets: data.snippets.length,
    forwards: data.forwards.length,
    known: data.knownHosts.length,
  });

  // ---------------------------------------------------------------- 启动
  $effect(() => {
    void (async () => {
      await loadAll();
      await listAddons().catch(() => []);
      await refreshSync(true);
    })();
    const unlisten = listen<string>('menu', (e) => {
      if (e.payload === 'close-tab' && activeKey) closeTab(activeKey);
      else if (e.payload === 'new-host') editing = { host: api.blankHost(), isNew: true };
      else if (e.payload === 'settings') settingsTab = 'appearance';
    });
    const timer = setInterval(() => void refreshSync(true), 5 * 60 * 1000);
    return () => {
      void unlisten.then((f) => f());
      clearInterval(timer);
    };
  });

  $effect(() => {
    document.documentElement.dataset.theme = prefs.theme;
    void applyFontPack(prefs.fontPack, { ...prefs.fontOverrides });
    void prefs.terminalFontSize;
    void prefs.syncSecrets;
    persistPrefs();
  });

  // ---------------------------------------------------------------- 同步
  async function refreshSync(run: boolean) {
    if (data.vaultError) return;
    try {
      const c = await api.syncCheck();
      if (!c.configured) {
        sync = { title: '同步未开启', detail: '点击设置', state: 'off' };
        return;
      }
      if (c.needPassword !== null) {
        sync = { title: '需要主密码', detail: '点击完成设置', state: 'err' };
        return;
      }
      if (!run) return;
      sync = { ...sync, title: '正在同步', state: 'busy' };
      const r = await api.syncNow();
      const t = new Date();
      sync = {
        title: '已同步',
        detail: `${String(t.getHours()).padStart(2, '0')}:${String(t.getMinutes()).padStart(2, '0')} · ↓${r.pulled} ↑${r.pushed}`,
        state: 'ok',
      };
      if (r.pulled) await loadAll();
    } catch (e) {
      sync = { title: '同步失败', detail: api.errorText(e).slice(0, 40), state: 'err' };
    }
  }

  function onSyncClick() {
    if (sync.state === 'off' || sync.state === 'err') settingsTab = 'sync';
    else void refreshSync(true);
  }

  // ---------------------------------------------------------------- 标签
  function openTab(host: Host) {
    const key = crypto.randomUUID();
    tabs.push({ key, host, status: { state: 'connecting' }, sftp: false });
    activeKey = key;
    view = 'hosts';
  }

  function closeTab(key: string) {
    const i = tabs.findIndex((t) => t.key === key);
    if (i < 0) return;
    tabs.splice(i, 1);
    delete panes[key];
    if (activeKey === key) activeKey = tabs[Math.min(i, tabs.length - 1)]?.key ?? null;
  }

  function sendSnippet(s: Snippet) {
    if (!activeKey) return;
    panes[activeKey]?.send(s.command.replace(/\n/g, '\r') + (s.run ? '\r' : ''));
    snippetMenu = false;
    view = 'hosts';
  }

  // ---------------------------------------------------------------- 主机
  async function save(h: Host) {
    await api.save('host', h);
    await loadAll();
    for (const t of tabs) if (t.host.id === h.id) t.host = h;
    editing = null;
  }

  async function remove(h: Host) {
    await api.remove('host', h.id);
    await loadAll();
    editing = null;
  }

  function onKeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (!mod) return;
    if (e.key === 'k') {
      e.preventDefault();
      view = 'hosts';
      searchInput?.focus();
    } else if (/^[1-9]$/.test(e.key) && e.metaKey) {
      const t = tabs[Number(e.key) - 1];
      if (t) {
        e.preventDefault();
        activeKey = t.key;
        view = 'hosts';
      }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if data.ready && data.vaultError}
  <div class="fatal">
    <Logo size={48} />
    <h1>无法打开本机 Vault</h1>
    <p>{data.vaultError}</p>
    <p class="mut">
      通常是系统钥匙串里的数据密钥被删除了。你可以重置本机数据；如果之前开启过云同步，重置后用主密码重新同步即可恢复。
    </p>
    <button class="btn danger" type="button" onclick={async () => { await api.vaultReset(); await loadAll(); }}>重置本机数据</button>
  </div>
{:else}
  <div class="app">
    <Sidebar
      hosts={data.hosts}
      connected={connectedIds}
      {macInset}
      {view}
      {counts}
      {sync}
      bind:searchInput
      onview={(v) => (view = v)}
      onconnect={openTab}
      onedit={(h) => (editing = { host: h, isNew: false })}
      onnew={() => (editing = { host: api.blankHost(), isNew: true })}
      onsettings={() => (settingsTab = 'appearance')}
      onsync={onSyncClick}
    />

    <main>
      <div class="tb-wrap">
        <TabBar
          tabs={tabs.map((t) => ({ key: t.key, title: t.host.name, state: t.status.state }))}
          activeKey={view === 'hosts' ? activeKey : null}
          onselect={(k) => {
            activeKey = k;
            view = 'hosts';
          }}
          onclose={closeTab}
        />
        {#if activeTab && view === 'hosts' && isTerminal(activeTab.host)}
          <div class="tools">
            <div class="snip">
              <button class="btn ghost" type="button" aria-expanded={snippetMenu} onclick={() => (snippetMenu = !snippetMenu)} disabled={!data.snippets.length}>
                <Icon name="code" size={14} />片段
              </button>
              {#if snippetMenu}
                <div class="menu" role="menu">
                  {#each data.snippets as s (s.id)}
                    <button type="button" role="menuitem" onclick={() => sendSnippet(s)}><b>{s.name}</b><small class="mono">{s.command}</small></button>
                  {/each}
                </div>
              {/if}
            </div>
            {#if activeTab.host.protocol === 'ssh'}
              <button
                class="btn ghost"
                class:on={activeTab.sftp}
                type="button"
                disabled={!activeTab.status.sessionId}
                onclick={() => activeTab && (activeTab.sftp = !activeTab.sftp)}
              >
                <Icon name="folder" size={14} />SFTP
              </button>
            {/if}
          </div>
        {/if}
      </div>

      <div class="stage" class:focus={view === 'hosts' && !!activeTab}>
        {#each tabs as t (t.key)}
          <div class="ws" class:hidden={view !== 'hosts' || t.key !== activeKey}>
            <div class="term-area">
              {#if isTerminal(t.host)}
                <TerminalPane
                  bind:this={panes[t.key]}
                  host={t.host}
                  active={view === 'hosts' && t.key === activeKey}
                  onstatus={(s) => (t.status = s)}
                  onhostkey={() => void loadAll()}
                />
              {:else}
                <RemoteDesktopPane host={t.host} active={view === 'hosts' && t.key === activeKey} onstatus={(s) => (t.status = s)} />
              {/if}
            </div>
            {#if t.sftp && t.status.sessionId}
              <div class="sftp-area"><SftpPanel sessionId={t.status.sessionId} onclose={() => (t.sftp = false)} /></div>
            {/if}
          </div>
        {/each}

        {#if view === 'keys'}
          <KeysView />
        {:else if view === 'snippets'}
          <SnippetsView onsend={activeTab && isTerminal(activeTab.host) && activeTab.status.state === 'connected' ? sendSnippet : null} />
        {:else if view === 'forwards'}
          <ForwardsView />
        {:else if view === 'known'}
          <KnownHostsView />
        {:else if tabs.length === 0}
          <div class="empty">
            <Logo size={56} />
            {#if data.hosts.length === 0}
              <h1>欢迎使用 Vornel</h1>
              <p>添加第一台主机，开始连接。</p>
              <button class="btn" type="button" onclick={() => (editing = { host: api.blankHost(), isNew: true })}>
                <Icon name="plus" size={14} stroke={2} />新建主机
              </button>
            {:else}
              <h1>选择一台主机开始连接</h1>
              <p>在左侧点击主机即可打开新会话。</p>
            {/if}
            <div class="keys">
              <span><kbd>⌘K</kbd> 搜索</span>
              <span><kbd>⌘N</kbd> 新建主机</span>
              <span><kbd>⌘W</kbd> 关闭标签</span>
              <span><kbd>⌘,</kbd> 设置</span>
            </div>
          </div>
        {/if}
      </div>

      <StatusBar host={view === 'hosts' ? (activeTab?.host ?? null) : null} status={view === 'hosts' ? (activeTab?.status ?? null) : null} />
    </main>
  </div>
{/if}

{#if editing}
  <HostDialog host={editing.host} isNew={editing.isNew} onsave={save} ondelete={remove} onclose={() => (editing = null)} />
{/if}

{#if settingsTab}
  <SettingsDialog initialTab={settingsTab} onclose={() => { settingsTab = null; void refreshSync(false); }} />
{/if}

<style>
  .app {
    height: 100%;
    display: flex;
  }
  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--win);
  }
  .tb-wrap {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--line);
  }
  .tb-wrap :global(.tb) {
    flex: 1;
    min-width: 0;
    border-bottom: 0;
  }
  .tools {
    display: flex;
    gap: 6px;
    padding-right: 10px;
  }
  .tools .btn {
    height: 28px;
  }
  .tools .btn.on {
    border-color: var(--acc);
    color: var(--acc);
  }
  .snip {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 34px;
    z-index: 20;
    width: 320px;
    max-height: 360px;
    overflow: auto;
    padding: 6px;
    border-radius: 10px;
    background: var(--raise);
    box-shadow: var(--shadow);
  }
  .menu button {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 7px;
    background: none;
    text-align: left;
  }
  .menu button:hover {
    background: var(--sel);
  }
  .menu small {
    font-size: 11.5px;
    color: var(--mut);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    margin: 6px;
    border-radius: 9px;
    overflow: hidden;
    background: var(--pane);
    box-shadow: 0 0 0 1px var(--line);
  }
  .stage.focus {
    box-shadow: 0 0 0 1px var(--acc);
  }
  .ws {
    position: absolute;
    inset: 0;
    display: flex;
  }
  .ws.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .term-area {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .sftp-area {
    width: min(620px, 50%);
    border-left: 1px solid var(--line);
  }
  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
    color: var(--mut);
  }
  .empty h1 {
    margin: 12px 0 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--fg);
  }
  .empty p {
    margin: 0 0 8px;
  }
  .keys {
    display: flex;
    gap: 18px;
    margin-top: 18px;
    font-size: 12px;
  }
  kbd {
    font: 500 11px var(--f-mono);
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--line2);
    margin-right: 4px;
  }
  .fatal {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 32px;
    text-align: center;
  }
  .fatal h1 {
    margin: 0;
    font-size: 20px;
  }
  .fatal p {
    margin: 0;
    max-width: 520px;
    line-height: 1.6;
  }
  .fatal .mut {
    color: var(--mut);
    font-size: 13px;
  }
</style>
