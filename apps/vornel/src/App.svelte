<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { tick, untrack } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import * as api from './lib/api';
  import type { Host, Snippet } from './lib/api';
  import { listAddons } from './lib/addons';
  import { applyFontPack } from './lib/fonts';
  import { data, loadAll, persistPrefs, prefs } from './lib/store.svelte';
  import { checkForUpdate, installUpdate, updater, updaterSupported } from './lib/updater.svelte';
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
  import KeyBar from './components/KeyBar.svelte';
  import MobileHosts from './components/MobileHosts.svelte';
  import KeysView from './views/KeysView.svelte';
  import KnownHostsView from './views/KnownHostsView.svelte';
  import SnippetsView from './views/SnippetsView.svelte';
  import HostsView from './views/HostsView.svelte';
  import WindowControls from './components/WindowControls.svelte';

  interface Tab {
    key: string;
    host: Host;
    status: PaneStatus;
    sftp: boolean;
  }

  // macOS 使用 Overlay 标题栏，红绿灯落在侧边栏左上角，需要留出位置
  const macInset = /Macintosh|Mac OS X/.test(navigator.userAgent) && !/iPhone|iPad/.test(navigator.userAgent);
  /** Windows 使用无边框窗口，顶栏右侧绘制窗口按钮 */
  const isWindows = /Windows/.test(navigator.userAgent);
  const isTerminal = (h: Host) => ['ssh', 'mosh', 'telnet', 'serial'].includes(h.protocol);

  // iOS 或窄窗口使用移动布局（DESIGN.md 5.4）
  const isIOS = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  let narrow = $state(window.innerWidth < 760);
  const mobile = $derived(isIOS || narrow);
  /** 移动布局下是否正在看某个会话（否则显示主机列表等页面） */
  let showSession = $state(false);
  let ctrlOn = $state(false);
  /** 偏好里用到、但本机还没下载的中文增强包 */
  let missingAddons = $state<string[]>([]);
  const MOBILE_TABS: { id: View | 'settings'; label: string; icon: 'host' | 'key' | 'code' | 'forward' | 'settings' }[] = [
    { id: 'hosts', label: '主机', icon: 'host' },
    { id: 'keys', label: '密钥串', icon: 'key' },
    { id: 'snippets', label: '片段', icon: 'code' },
    { id: 'forwards', label: '转发', icon: 'forward' },
    { id: 'settings', label: '设置', icon: 'settings' },
  ];

  let view = $state<View>('hosts');
  let tabs = $state<Tab[]>([]);
  let activeKey = $state<string | null>(null);
  let editing = $state<{ host: Host; isNew: boolean } | null>(null);
  let settingsTab = $state<'appearance' | 'sync' | null>(null);
  let searchInput = $state<HTMLInputElement>();
  let snippetMenu = $state(false);
  const panes: Record<string, { send: (text: string) => void; setCtrl: (on: boolean) => void } | undefined> = $state({});

  let sync = $state<{ title: string; detail: string; state: 'off' | 'ok' | 'busy' | 'err' }>({
    title: '同步未开启',
    detail: '点击设置',
    state: 'off',
  });

  const activeTab = $derived(tabs.find((t) => t.key === activeKey) ?? null);
  /** 桌面端沉浸模式（参考 Termius）：打开会话后隐藏侧边栏与状态栏，终端铺满窗口，顶部只留标签栏 */
  const immersive = $derived(!mobile && showSession && view === 'hosts' && !!activeTab);
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
      const addons = await listAddons().catch(() => []);
      // 新设备同步了偏好，但还没下载增强包：先用系统字体，并提示下载（ARCHITECTURE.md 8.3）
      missingAddons = addons
        .filter((a) => !a.installed && Object.values(prefs.fontOverrides).includes(a.family))
        .map((a) => a.family);
      await refreshSync(true);
    })();
    // 启动后静默检查一次更新（开发模式下不检查）
    if (updaterSupported && !import.meta.env.DEV) untrack(() => void checkForUpdate(true));
    const unlisten = listen<string>('menu', (e) => {
      if (e.payload === 'close-tab' && activeKey) closeTab(activeKey);
      else if (e.payload === 'new-host') editing = { host: api.blankHost(), isNew: true };
      else if (e.payload === 'settings') settingsTab = 'appearance';
    });
    const timer = setInterval(() => void refreshSync(true), 5 * 60 * 1000);
    const onResize = () => (narrow = window.innerWidth < 760);
    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
      void unlisten.then((f) => f());
      clearInterval(timer);
    };
  });

  $effect(() => {
    document.documentElement.dataset.theme = prefs.theme;
    void applyFontPack(prefs.fontPack, { ...prefs.fontOverrides });
  });
  // 任何偏好变化（含选中即复制、屏幕阅读器、分组外观等）都写回 Vault
  $effect(() => {
    void JSON.stringify(prefs);
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
    showSession = true;
  }

  /** 搜索框里输入 ssh user@host：保存为一台新主机（同地址同用户的已有主机直接复用）并连接 */
  async function quickConnect(t: { username: string; address: string; port: number }) {
    const existing = data.hosts.find((h) => h.address === t.address && h.username === t.username && h.port === t.port);
    if (existing) return openTab(existing);
    const h: Host = { ...api.blankHost(), name: `${t.username}@${t.address}`, address: t.address, port: t.port, username: t.username };
    await api.save('host', h);
    await loadAll();
    openTab(h);
  }

  function closeTab(key: string) {
    const i = tabs.findIndex((t) => t.key === key);
    if (i < 0) return;
    tabs.splice(i, 1);
    delete panes[key];
    if (activeKey === key) activeKey = tabs[Math.min(i, tabs.length - 1)]?.key ?? null;
    if (!tabs.length) showSession = false;
  }

  function sendSnippet(s: Snippet) {
    if (!activeKey) return;
    panes[activeKey]?.send(s.command.replace(/\n/g, '\r') + (s.run ? '\r' : ''));
    snippetMenu = false;
    view = 'hosts';
  }

  // ---------------------------------------------------------------- 主机
  async function save(h: Host, password?: string) {
    await api.save('host', h);
    // 编辑主机时直接填写的密码：加密保存在 Vault，连接时不再询问
    if (password) await api.save('secret', { id: h.id, password, sync: false });
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
    if (e.key === '=' || e.key === '+') {
      e.preventDefault();
      prefs.terminalFontSize = Math.min(24, prefs.terminalFontSize + 1);
    } else if (e.key === '-') {
      e.preventDefault();
      prefs.terminalFontSize = Math.max(10, prefs.terminalFontSize - 1);
    } else if (e.key === '0') {
      e.preventDefault();
      prefs.terminalFontSize = 13;
    } else if (e.key === 'b' && !mobile && tabs.length) {
      e.preventDefault();
      showSession = !showSession;
      if (showSession) view = 'hosts';
    } else if (!macInset && e.key === 'n') {
      e.preventDefault();
      editing = { host: api.blankHost(), isNew: true };
    } else if (!macInset && e.key === 'w') {
      e.preventDefault();
      if (activeKey) closeTab(activeKey);
    } else if (!macInset && e.key === ',') {
      e.preventDefault();
      settingsTab = 'appearance';
    } else if (e.key === 'k') {
      e.preventDefault();
      showSession = false;
      view = 'hosts';
      void tick().then(() => searchInput?.focus());
    } else if (/^[1-9]$/.test(e.key) && e.metaKey) {
      const t = tabs[Number(e.key) - 1];
      if (t) {
        e.preventDefault();
        activeKey = t.key;
        view = 'hosts';
        showSession = true;
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
  <div class="app" class:mobile>
    {#if !mobile && !immersive}
    <div class="sbw" in:fly={{ x: -28, duration: 200, easing: cubicOut }}>
    <Sidebar
      {macInset}
      {view}
      {counts}
      {sync}
      onview={(v) => {
        view = v;
        if (v === 'hosts') showSession = false;
      }}
      onnew={() => (editing = { host: api.blankHost(), isNew: true })}
      onsettings={() => (settingsTab = 'appearance')}
      onsync={onSyncClick}
    />
    </div>
    {/if}

    <main>
      {#if mobile && showSession && activeTab}
        <div class="mnav">
          <button class="back" type="button" onclick={() => (showSession = false)}>
            <span class="flip"><Icon name="chevron" size={18} stroke={2.2} /></span>主机
          </button>
          <div class="mid">
            <b>{activeTab.host.name}</b>
            <small class:ok={activeTab.status.state === 'connected'}>
              {activeTab.status.state === 'connected' ? '● 已连接' : activeTab.status.state === 'connecting' ? '正在连接' : '已断开'}
            </small>
          </div>
          {#if activeTab.host.protocol === 'ssh'}
            <button class="icon-btn big" type="button" aria-label="SFTP" disabled={!activeTab.status.sessionId} onclick={() => activeTab && (activeTab.sftp = !activeTab.sftp)}>
              <Icon name="folder" size={20} />
            </button>
          {:else}
            <span class="big"></span>
          {/if}
        </div>
      {/if}
      {#if !mobile}
      <div class="tb-wrap" class:immersive class:mac={immersive && macInset} data-tauri-drag-region>
        {#if immersive}
          <button class="home-btn" type="button" title="回到主机列表 ⌘B" onclick={() => (showSession = false)}>
            <Icon name="host" size={15} />主机
          </button>
        {/if}
        <TabBar
          tabs={tabs.map((t) => ({ key: t.key, title: t.host.name, state: t.status.state }))}
          activeKey={view === 'hosts' ? activeKey : null}
          onselect={(k) => {
            activeKey = k;
            view = 'hosts';
            showSession = true;
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
        {#if isWindows}<WindowControls />{/if}
      </div>
      {/if}

      {#if missingAddons.length}
        <div class="notice" role="status">
          <span>此设备还没有字体增强包「{missingAddons.join('」「')}」，暂时使用系统字体。</span>
          <button class="btn ghost" type="button" onclick={() => (settingsTab = 'appearance')}>去下载</button>
          <button class="icon-btn" type="button" aria-label="关闭提示" onclick={() => (missingAddons = [])}><Icon name="close" size={14} /></button>
        </div>
      {/if}
      {#if updater.update && !updater.dismissed}
        <div class="notice" role="status">
          {#if updater.downloading}
            <span>正在下载 Vornel {updater.update.version}{updater.progress === null ? '…' : `（${updater.progress}%）`}，完成后自动重启。</span>
          {:else}
            <span>{updater.error || `发现新版本 ${updater.update.version}`}</span>
            <button class="btn" type="button" onclick={installUpdate}>更新并重启</button>
            <button class="icon-btn" type="button" aria-label="关闭提示" onclick={() => (updater.dismissed = true)}><Icon name="close" size={14} /></button>
          {/if}
        </div>
      {/if}
      <div class="stage" class:focus={view === 'hosts' && !!activeTab} class:immersive>
        {#each tabs as t (t.key)}
          <div class="ws" class:hidden={view !== 'hosts' || t.key !== activeKey || !showSession}>
            <div class="term-area">
              {#if isTerminal(t.host)}
                <TerminalPane
                  bind:this={panes[t.key]}
                  autoReconnect={isIOS}
                  onctrlused={() => (ctrlOn = false)}
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
        {:else if mobile && !showSession}
          <MobileHosts
            hosts={data.hosts}
            sessions={tabs.map((t) => ({ key: t.key, title: t.host.name, state: t.status.state }))}
            onconnect={openTab}
            onresume={(k) => {
              activeKey = k;
              showSession = true;
            }}
            onedit={(h) => (editing = { host: h, isNew: false })}
            onnew={() => (editing = { host: api.blankHost(), isNew: true })}
            onknown={() => (view = 'known')}
          />
        {:else if view === 'hosts' && (!showSession || !activeTab)}
          <HostsView
            hosts={data.hosts}
            connected={connectedIds}
            bind:searchInput
            onconnect={openTab}
            onedit={(h) => (editing = { host: h, isNew: false })}
            onduplicate={(h) => (editing = { host: { ...$state.snapshot(h), id: crypto.randomUUID(), name: `${h.name} 副本`, savePassword: false }, isNew: true })}
            onnew={(g) => (editing = { host: { ...api.blankHost(), group: g }, isNew: true })}
            onquick={quickConnect}
          />
        {/if}
      </div>

      {#if mobile}
        {#if showSession && activeTab && view === 'hosts' && isTerminal(activeTab.host)}
          <KeyBar
            ctrl={ctrlOn}
            onctrl={(on) => {
              ctrlOn = on;
              if (activeKey) panes[activeKey]?.setCtrl(on);
            }}
            onkey={(seq) => activeKey && panes[activeKey]?.send(seq)}
          />
        {:else if !showSession}
          <nav class="mtabs" aria-label="主导航">
            {#each MOBILE_TABS as mt (mt.id)}
              <button
                type="button"
                class:on={mt.id === view}
                aria-current={mt.id === view ? 'page' : undefined}
                onclick={() => (mt.id === 'settings' ? (settingsTab = 'appearance') : (view = mt.id))}
              >
                <Icon name={mt.icon} size={23} />{mt.label}
              </button>
            {/each}
          </nav>
        {/if}
      {:else}
        {#if !immersive}
          <StatusBar host={view === 'hosts' ? (activeTab?.host ?? null) : null} status={view === 'hosts' ? (activeTab?.status ?? null) : null} />
        {/if}
      {/if}
    </main>
  </div>
{/if}

{#if editing}
  <HostDialog host={editing.host} isNew={editing.isNew} onsave={save} ondelete={remove} onclose={() => (editing = null)} />
{/if}

{#if settingsTab}
  <SettingsDialog initialTab={settingsTab} onclose={() => { settingsTab = null; void refreshSync(true); }} />
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
  /* 沉浸模式：终端贴边铺满，顶部标签栏为红绿灯留位 */
  .stage.immersive {
    margin: 0;
    border-radius: 0;
    box-shadow: none;
  }
  .tb-wrap.immersive {
    background: var(--pane);
  }
  .tb-wrap.mac {
    padding-left: 84px;
  }
  .home-btn {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    min-width: 84px;
    justify-content: center;
    margin-left: 6px;
    padding: 0 14px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--mut);
    font-size: 13px;
    flex: none;
  }
  .home-btn:hover {
    background: var(--sel);
    color: var(--fg);
  }
  .stage.focus {
    box-shadow: 0 0 0 1px var(--acc);
  }
  .ws {
    position: absolute;
    inset: 0;
    display: flex;
  }
  /* 切到会话时淡入 */
  .ws:not(.hidden) {
    animation: ws-in 180ms var(--ease) both;
  }
  @keyframes ws-in {
    from {
      opacity: 0;
      transform: scale(0.995);
    }
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
  .sbw {
    display: flex;
    flex: none;
  }
  .sftp-area {
    width: min(620px, 50%);
    border-left: 1px solid var(--line);
  }
  .notice {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 6px 6px 0;
    padding: 6px 6px 6px 12px;
    border-radius: 8px;
    background: var(--accbg);
    font-size: 12.5px;
  }
  .notice span {
    flex: 1;
  }
  .notice .btn {
    height: 26px;
  }

  /* ---------------- 移动布局 ---------------- */
  .app.mobile main {
    padding-top: env(safe-area-inset-top, 0px);
  }
  .app.mobile .stage {
    margin: 0;
    border-radius: 0;
    box-shadow: none;
  }
  .app.mobile .sftp-area {
    position: absolute;
    inset: 0;
    width: auto;
    z-index: 5;
    border: 0;
  }
  .mnav {
    height: 48px;
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 6px;
    border-bottom: 1px solid var(--line);
  }
  .back {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 44px;
    min-width: 72px;
    padding: 0 6px;
    border: 0;
    background: none;
    color: var(--acc);
    font-size: 16px;
  }
  .flip :global(svg) {
    transform: rotate(180deg);
  }
  .mid {
    flex: 1;
    text-align: center;
    min-width: 0;
  }
  .mid b {
    display: block;
    font: 600 14.5px var(--f-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mid small {
    font-size: 11.5px;
    color: var(--mut);
  }
  .mid small.ok {
    color: var(--ok);
  }
  .big {
    width: 72px;
    display: flex;
    justify-content: flex-end;
  }
  .icon-btn.big {
    width: 44px;
    height: 44px;
    margin-left: 28px;
    color: var(--acc);
  }
  .mtabs {
    display: flex;
    justify-content: space-around;
    padding: 6px 0 calc(6px + env(safe-area-inset-bottom, 0px));
    border-top: 1px solid var(--line);
    background: var(--sb);
  }
  .mtabs button {
    min-width: 64px;
    min-height: 46px;
    border: 0;
    background: none;
    color: var(--mut);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    font-size: 10.5px;
    font-weight: 500;
  }
  .mtabs button.on {
    color: var(--acc);
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
