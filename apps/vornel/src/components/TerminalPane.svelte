<script lang="ts" module>
  export type PaneState = 'auth' | 'connecting' | 'connected' | 'closed' | 'error';
  export interface PaneStatus {
    state: PaneState;
    message?: string;
    cols?: number;
    rows?: number;
  }
</script>

<script lang="ts">
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebglAddon } from '@xterm/addon-webgl';
  import { untrack } from 'svelte';
  import { errorText, sshClose, sshConnect, sshResize, sshWrite, type Host } from '../lib/api';
  import { ensureFamily, familyStack, findPack } from '../lib/fonts';
  import { settings } from '../lib/settings.svelte';
  import { badge, TERMINAL_THEMES } from '../lib/themes';

  let {
    host,
    active,
    onstatus,
  }: { host: Host; active: boolean; onstatus: (s: PaneStatus) => void } = $props();

  let container = $state<HTMLDivElement>();
  let term: Terminal | undefined;
  let fit: FitAddon | undefined;
  let webgl: WebglAddon | undefined;
  let sessionId: string | null = null;

  let pane = $state<PaneStatus>({ state: 'connecting' });
  let password = $state('');
  let passphrase = $state('');
  let showPassphrase = $state(false);
  let passwordInput = $state<HTMLInputElement>();

  function setStatus(s: PaneStatus) {
    pane = s;
    onstatus({ ...s, cols: term?.cols, rows: term?.rows });
  }

  function target() {
    return `${host.username}@${host.address}:${host.port}`;
  }

  function line(kind: 'ssh' | 'key' | 'err' | 'info', label: string, text: string) {
    term?.write(badge(settings.theme, kind, label, text));
  }

  async function connect() {
    if (!term) return;
    setStatus({ state: 'connecting' });
    line('info', '···', `正在连接 ${target()}`);
    try {
      sessionId = await sshConnect({
        hostId: host.id,
        password: host.auth.type === 'password' ? password : undefined,
        passphrase: passphrase || undefined,
        cols: term.cols,
        rows: term.rows,
        onData: (bytes) => term?.write(bytes),
        onEvent: (ev) => {
          if (ev.type === 'connected') {
            line('ssh', 'SSH', `已连接 ${target()}`);
            line('key', 'KEY', `${ev.fingerprint}${ev.newHostKey ? '  首次连接，已记录此指纹，请核对' : ''}`);
            setStatus({ state: 'connected' });
          } else {
            sessionId = null;
            line('info', 'END', `连接已关闭${ev.exitStatus != null ? `（退出码 ${ev.exitStatus}）` : ''}`);
            setStatus({ state: 'closed' });
          }
        },
      });
      password = '';
      term.focus();
    } catch (e) {
      sessionId = null;
      const message = errorText(e);
      line('err', 'ERR', message);
      if (host.auth.type === 'keyFile' && /passphrase|decrypt|加密|口令/i.test(message)) {
        showPassphrase = true;
      }
      setStatus({ state: 'error', message });
    }
  }

  function submitAuth(e: SubmitEvent) {
    e.preventDefault();
    void connect();
  }

  async function applyAppearance() {
    if (!term) return;
    const pack = findPack(settings.fontPack);
    // 顺序很重要：先等字体真正加载，再改字体、清字形缓存、重新计算行列
    await ensureFamily(pack.terminal);
    term.options.fontFamily = familyStack(pack.terminal, 'monospace');
    term.options.fontSize = settings.terminalFontSize;
    term.options.lineHeight = pack.lineHeight;
    term.options.theme = TERMINAL_THEMES[settings.theme];
    webgl?.clearTextureAtlas();
    refit();
  }

  function refit() {
    if (!term || !fit || !container?.offsetWidth) return;
    const before = `${term.cols}x${term.rows}`;
    fit.fit();
    if (`${term.cols}x${term.rows}` !== before) {
      onstatus({ ...pane, cols: term.cols, rows: term.rows });
      if (sessionId) void sshResize(sessionId, term.cols, term.rows).catch(() => {});
    }
  }

  // 创建终端（只执行一次）
  $effect(() => {
    const el = container;
    if (!el) return;
    let disposed = false;
    const pack = untrack(() => findPack(settings.fontPack));

    (async () => {
      await ensureFamily(pack.terminal);
      if (disposed) return;
      term = new Terminal({
        fontFamily: familyStack(pack.terminal, 'monospace'),
        fontSize: untrack(() => settings.terminalFontSize),
        lineHeight: pack.lineHeight,
        theme: TERMINAL_THEMES[untrack(() => settings.theme)],
        cursorBlink: true,
        scrollback: 10000,
        macOptionIsMeta: true,
        allowTransparency: false,
      });
      fit = new FitAddon();
      term.loadAddon(fit);
      term.open(el);
      try {
        webgl = new WebglAddon();
        webgl.onContextLoss(() => {
          webgl?.dispose();
          webgl = undefined;
        });
        term.loadAddon(webgl);
      } catch {
        webgl = undefined; // 不支持 WebGL 时退回 DOM 渲染
      }
      fit.fit();
      term.onData((data) => {
        if (sessionId) void sshWrite(sessionId, data).catch(() => {});
      });

      if (untrack(() => host.auth.type) === 'password') {
        setStatus({ state: 'auth' });
      } else {
        void connect();
      }
    })();

    const ro = new ResizeObserver(() => refit());
    ro.observe(el);

    return () => {
      disposed = true;
      ro.disconnect();
      if (sessionId) void sshClose(sessionId).catch(() => {});
      sessionId = null;
      webgl?.dispose();
      term?.dispose();
    };
  });

  // 主题、字体包、字号变化时更新终端
  $effect(() => {
    void settings.theme;
    void settings.fontPack;
    void settings.terminalFontSize;
    void applyAppearance();
  });

  // 切到这个标签时聚焦
  $effect(() => {
    if (active) {
      requestAnimationFrame(() => {
        refit();
        if (untrack(() => pane.state) === 'auth') passwordInput?.focus();
        else term?.focus();
      });
    }
  });
</script>

<div class="pane" class:hidden={!active} role="application" aria-label="终端 {host.name}">
  <div class="term" bind:this={container}></div>

  {#if pane.state === 'auth' || pane.state === 'error' || pane.state === 'closed'}
    <div class="overlay">
      <form class="card" onsubmit={submitAuth}>
        <div class="title">
          <span class="mono">{target()}</span>
          {#if pane.state === 'error'}<span class="hint err">连接失败</span>{/if}
          {#if pane.state === 'closed'}<span class="hint">连接已关闭</span>{/if}
        </div>
        {#if host.auth.type === 'password'}
          <label class="field">
            <span>密码</span>
            <input
              class="input"
              type="password"
              autocomplete="current-password"
              bind:value={password}
              bind:this={passwordInput}
            />
          </label>
        {:else if showPassphrase}
          <label class="field">
            <span>私钥口令</span>
            <input class="input" type="password" bind:value={passphrase} />
          </label>
        {/if}
        <button class="btn" type="submit" disabled={host.auth.type === 'password' && !password}>
          {pane.state === 'auth' ? '连接' : '重新连接'}
        </button>
      </form>
    </div>
  {/if}
</div>

<style>
  .pane {
    position: absolute;
    inset: 0;
    display: flex;
    background: var(--pane);
  }
  .pane.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .term {
    flex: 1;
    min-width: 0;
    padding: 12px 6px 6px 14px;
  }
  .term :global(.xterm) {
    height: 100%;
  }
  .term :global(.xterm-viewport) {
    background: transparent !important;
  }
  .overlay {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--pane) 70%, transparent);
    backdrop-filter: blur(2px);
  }
  .card {
    width: min(360px, calc(100% - 32px));
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 20px;
    border-radius: 12px;
    background: var(--raise);
    box-shadow: var(--shadow);
  }
  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
  }
  .hint {
    font-size: 12px;
    color: var(--mut);
  }
  .hint.err {
    color: var(--err);
  }
  .card .btn {
    height: 34px;
  }
</style>
