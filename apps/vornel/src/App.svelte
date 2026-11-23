<script lang="ts">
  import { blankHost, errorText, hostsDelete, hostsList, hostsSave, type Host } from './lib/api';
  import { applyFontPack } from './lib/fonts';
  import { saveSettings, settings } from './lib/settings.svelte';
  import HostDialog from './components/HostDialog.svelte';
  import Icon from './components/Icon.svelte';
  import Logo from './components/Logo.svelte';
  import SettingsDialog from './components/SettingsDialog.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import TabBar from './components/TabBar.svelte';
  import TerminalPane, { type PaneStatus } from './components/TerminalPane.svelte';

  interface Tab {
    key: string;
    host: Host;
    status: PaneStatus;
  }

  // macOS 使用 Overlay 标题栏，红绿灯落在侧边栏左上角，需要留出位置
  const macInset = /Macintosh|Mac OS X/.test(navigator.userAgent) && !/iPhone|iPad/.test(navigator.userAgent);

  let hosts = $state<Host[]>([]);
  let loadError = $state('');
  let tabs = $state<Tab[]>([]);
  let activeKey = $state<string | null>(null);
  let editing = $state<{ host: Host; isNew: boolean } | null>(null);
  let showSettings = $state(false);
  let searchInput = $state<HTMLInputElement>();

  const activeTab = $derived(tabs.find((t) => t.key === activeKey) ?? null);
  const connectedIds = $derived(new Set(tabs.filter((t) => t.status.state === 'connected').map((t) => t.host.id)));
  const groups = $derived([...new Set(hosts.map((h) => h.group).filter((g): g is string => !!g))].sort());

  async function reload() {
    try {
      hosts = await hostsList();
      loadError = '';
    } catch (e) {
      loadError = errorText(e);
    }
  }

  $effect(() => {
    void reload();
  });

  $effect(() => {
    document.documentElement.dataset.theme = settings.theme;
    saveSettings();
  });

  $effect(() => {
    void applyFontPack(settings.fontPack);
    void settings.terminalFontSize;
    saveSettings();
  });

  function openTab(host: Host) {
    const key = crypto.randomUUID();
    tabs.push({ key, host, status: { state: 'connecting' } });
    activeKey = key;
  }

  function closeTab(key: string) {
    const i = tabs.findIndex((t) => t.key === key);
    if (i < 0) return;
    tabs.splice(i, 1);
    if (activeKey === key) activeKey = tabs[Math.min(i, tabs.length - 1)]?.key ?? null;
  }

  async function save(h: Host) {
    await hostsSave(h);
    await reload();
    // 已打开的标签使用最新的主机信息（下次重连时生效）
    for (const t of tabs) if (t.host.id === h.id) t.host = h;
    editing = null;
  }

  async function remove(h: Host) {
    await hostsDelete(h.id);
    await reload();
    editing = null;
  }

  function onKeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (!mod) return;
    if (e.key === 'k') {
      e.preventDefault();
      searchInput?.focus();
    } else if (e.key === ',') {
      e.preventDefault();
      showSettings = true;
    } else if (e.key === 'n' && !e.shiftKey) {
      e.preventDefault();
      editing = { host: blankHost(), isNew: true };
    } else if (e.key === 'w' && activeKey && e.metaKey) {
      e.preventDefault();
      closeTab(activeKey);
    } else if (/^[1-9]$/.test(e.key) && e.metaKey) {
      const t = tabs[Number(e.key) - 1];
      if (t) {
        e.preventDefault();
        activeKey = t.key;
      }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <Sidebar
    {hosts}
    connected={connectedIds}
    {macInset}
    bind:searchInput
    onconnect={openTab}
    onedit={(h) => (editing = { host: h, isNew: false })}
    onnew={() => (editing = { host: blankHost(), isNew: true })}
    onsettings={() => (showSettings = true)}
  />

  <main>
    <TabBar
      tabs={tabs.map((t) => ({ key: t.key, title: t.host.name, state: t.status.state }))}
      {activeKey}
      onselect={(k) => (activeKey = k)}
      onclose={closeTab}
    />

    <div class="stage">
      {#each tabs as t (t.key)}
        <TerminalPane host={t.host} active={t.key === activeKey} onstatus={(s) => (t.status = s)} />
      {/each}

      {#if tabs.length === 0}
        <div class="empty">
          <Logo size={56} />
          {#if loadError}
            <p class="err">{loadError}</p>
          {:else if hosts.length === 0}
            <h1>欢迎使用 Vornel</h1>
            <p>添加第一台主机，开始连接。</p>
            <button class="btn" type="button" onclick={() => (editing = { host: blankHost(), isNew: true })}>
              <Icon name="plus" size={14} stroke={2} />新建主机
            </button>
          {:else}
            <h1>选择一台主机开始连接</h1>
            <p>在左侧点击主机即可打开新会话。</p>
          {/if}
          <div class="keys">
            <span><kbd>⌘K</kbd> 搜索</span>
            <span><kbd>⌘N</kbd> 新建主机</span>
            <span><kbd>⌘,</kbd> 设置</span>
          </div>
        </div>
      {/if}
    </div>

    <StatusBar host={activeTab?.host ?? null} status={activeTab?.status ?? null} />
  </main>
</div>

{#if editing}
  <HostDialog
    host={editing.host}
    isNew={editing.isNew}
    {groups}
    onsave={save}
    ondelete={remove}
    onclose={() => (editing = null)}
  />
{/if}

{#if showSettings}
  <SettingsDialog onclose={() => (showSettings = false)} />
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
  .stage:has(:global(.pane:not(.hidden))) {
    box-shadow: 0 0 0 1px var(--acc);
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
  .empty .err {
    color: var(--err);
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
</style>
