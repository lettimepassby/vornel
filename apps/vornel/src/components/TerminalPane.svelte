<script lang="ts" module>
  export type PaneState = 'auth' | 'connecting' | 'connected' | 'closed' | 'error';
  export interface PaneStatus {
    state: PaneState;
    message?: string;
    cols?: number;
    rows?: number;
    sessionId?: string | null;
  }
</script>

<script lang="ts">
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebglAddon } from '@xterm/addon-webgl';
  import { untrack } from 'svelte';
  import * as api from '../lib/api';
  import type { Host } from '../lib/api';
  import { ensureFamily, familyStack, findPack } from '../lib/fonts';
  import { data, prefs } from '../lib/store.svelte';
  import { badge, TERMINAL_THEMES, type BadgeKind } from '../lib/themes';

  let {
    host,
    active,
    onstatus,
    onhostkey,
    autoReconnect = false,
    onctrlused,
  }: {
    host: Host;
    active: boolean;
    onstatus: (s: PaneStatus) => void;
    /** 记录了新的主机指纹（界面需要刷新已知主机列表） */
    onhostkey?: () => void;
    /** 回到前台时自动重连（iOS 切到后台约 30 秒后会断网） */
    autoReconnect?: boolean;
    /** 粘滞 Ctrl 被使用后通知界面复位按钮 */
    onctrlused?: () => void;
  } = $props();

  let ctrlLatch = false;

  /** 移动端按键栏的粘滞 Ctrl：下一个输入的字母会变成控制字符。 */
  export function setCtrl(on: boolean) {
    ctrlLatch = on;
    term?.focus();
  }

  let container = $state<HTMLDivElement>();
  let term: Terminal | undefined;
  let fit: FitAddon | undefined;
  let webgl: WebglAddon | undefined;
  let sessionId: string | null = null;

  let pane = $state<PaneStatus>({ state: 'connecting' });
  let password = $state('');
  let savePassword = $state(false);
  let passphrase = $state('');
  let ask = $state<'password' | 'passphrase' | null>(null);
  let keyChanged = $state(false);
  let firstInput = $state<HTMLInputElement>();

  const terminalProtocols = ['ssh', 'mosh', 'telnet', 'serial'];

  function setStatus(s: PaneStatus) {
    pane = s;
    onstatus({ ...s, cols: term?.cols, rows: term?.rows, sessionId });
  }

  function target() {
    if (host.protocol === 'serial') return host.address;
    return `${host.username ? host.username + '@' : ''}${host.address}:${host.port}`;
  }

  function line(kind: BadgeKind, label: string, text: string) {
    term?.write(badge(prefs.theme, kind, label, text));
  }

  /** 把文字发送到终端（代码片段等）。 */
  export function send(text: string) {
    if (sessionId) void api.sessionWrite(sessionId, text).catch(() => {});
    term?.focus();
  }

  async function connect() {
    if (!term) return;
    keyChanged = false;
    setStatus({ state: 'connecting' });
    line('info', '···', `正在连接 ${target()}${host.jumpHost ? '（经由跳板机）' : ''}`);
    try {
      sessionId = await api.sessionOpen({
        hostId: host.id,
        password: ask === 'password' ? password : undefined,
        passphrase: passphrase || undefined,
        savePassword: ask === 'password' && savePassword,
        cols: term.cols,
        rows: term.rows,
        onData: (bytes) => term?.write(bytes),
        onEvent: (ev) => {
          if (ev.type === 'hostKey') {
            line('key', 'KEY', `${ev.host}:${ev.port}  ${ev.fingerprint}${ev.new ? '  首次连接，已记录，请核对' : ''}`);
            if (ev.new) onhostkey?.();
          } else if (ev.type === 'connected') {
            line('ssh', host.protocol.toUpperCase().slice(0, 4), `已连接 ${target()}`);
            // 经由这台主机正在运行的端口转发
            for (const f of data.forwards) {
              if (f.hostId !== host.id || !data.runningForwards.includes(f.id)) continue;
              const bind = `${f.bindAddress}:${f.bindPort}`;
              const text =
                f.kind === 'dynamic'
                  ? `SOCKS5 ${bind}`
                  : f.kind === 'local'
                    ? `${bind} → ${f.targetHost}:${f.targetPort}`
                    : `远端 ${bind} → 本机 ${f.targetHost}:${f.targetPort}`;
              line('fwd', 'FWD', `${f.name}  ${text}`);
            }
            setStatus({ state: 'connected' });
          } else {
            sessionId = null;
            const detail = ev.reason ? `：${ev.reason}` : ev.exitStatus != null ? `（退出码 ${ev.exitStatus}）` : '';
            line(ev.reason ? 'err' : 'info', 'END', `连接已关闭${detail}`);
            setStatus({ state: 'closed' });
          }
        },
      });
      password = '';
      ask = null;
      onstatus({ ...pane, sessionId, cols: term.cols, rows: term.rows });
      term.focus();
    } catch (e) {
      sessionId = null;
      const message = api.errorText(e);
      if (message.startsWith('NEED_PASSWORD')) {
        ask = 'password';
        setStatus({ state: 'auth' });
        return;
      }
      if (message.includes('私钥需要口令')) {
        ask = 'passphrase';
        setStatus({ state: 'auth' });
        return;
      }
      keyChanged = message.includes('主机密钥与已信任的不一致');
      line('err', 'ERR', message);
      setStatus({ state: 'error', message });
    }
  }

  async function trustNewKey() {
    await api.hostKeyTrust(host.address, host.port);
    onhostkey?.();
    await connect();
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    void connect();
  }

  async function applyAppearance() {
    if (!term) return;
    const pack = findPack(prefs.fontPack);
    const family = prefs.fontOverrides.terminal || pack.terminal;
    // 顺序很重要：先等字体真正加载，再改字体、清字形缓存、重新计算行列
    await ensureFamily(family);
    term.options.fontFamily = familyStack(family, 'monospace');
    term.options.fontSize = prefs.terminalFontSize;
    term.options.lineHeight = pack.lineHeight;
    term.options.theme = TERMINAL_THEMES[prefs.theme];
    term.options.screenReaderMode = prefs.screenReader;
    webgl?.clearTextureAtlas();
    refit();
  }

  function refit() {
    if (!term || !fit || !container?.offsetWidth) return;
    const before = `${term.cols}x${term.rows}`;
    fit.fit();
    if (`${term.cols}x${term.rows}` !== before) {
      onstatus({ ...pane, cols: term.cols, rows: term.rows, sessionId });
      if (sessionId) void api.sessionResize(sessionId, term.cols, term.rows).catch(() => {});
    }
  }

  // 创建终端（只执行一次）
  $effect(() => {
    const el = container;
    if (!el) return;
    let disposed = false;
    const pack = untrack(() => findPack(prefs.fontPack));
    const family = untrack(() => prefs.fontOverrides.terminal || pack.terminal);

    (async () => {
      await ensureFamily(family);
      if (disposed) return;
      term = new Terminal({
        fontFamily: familyStack(family, 'monospace'),
        fontSize: untrack(() => prefs.terminalFontSize),
        lineHeight: pack.lineHeight,
        theme: TERMINAL_THEMES[untrack(() => prefs.theme)],
        cursorBlink: true,
        scrollback: 10000,
        macOptionIsMeta: true,
        screenReaderMode: untrack(() => prefs.screenReader),
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
      term.onData((d) => {
        let out = d;
        if (ctrlLatch && d.length === 1) {
          const c = d.toUpperCase().charCodeAt(0);
          if (c >= 64 && c <= 95) out = String.fromCharCode(c - 64);
          ctrlLatch = false;
          onctrlused?.();
        }
        if (sessionId) void api.sessionWrite(sessionId, out).catch(() => {});
      });

      const h = untrack(() => host);
      if (!terminalProtocols.includes(h.protocol)) {
        line('err', 'ERR', '该协议不是终端会话');
        setStatus({ state: 'error' });
      } else if (h.protocol !== 'serial' && h.protocol !== 'telnet' && h.auth.type === 'password' && !h.savePassword) {
        ask = 'password';
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
      if (sessionId) void api.sessionClose(sessionId).catch(() => {});
      sessionId = null;
      webgl?.dispose();
      term?.dispose();
    };
  });

  // 主题、字体包、字号变化时更新终端
  $effect(() => {
    void prefs.theme;
    void prefs.fontPack;
    void prefs.terminalFontSize;
    void prefs.fontOverrides.terminal;
    void prefs.screenReader;
    void applyAppearance();
  });

  // 回到前台时，断开的会话自动重连（已保存凭据的情况下）
  $effect(() => {
    if (!autoReconnect) return;
    const onVisible = () => {
      if (document.visibilityState !== 'visible') return;
      const st = untrack(() => pane.state);
      const h = untrack(() => host);
      const needsPrompt = h.auth.type === 'password' && !h.savePassword && h.protocol !== 'telnet' && h.protocol !== 'serial';
      if ((st === 'closed' || st === 'error') && !needsPrompt && !keyChanged) void connect();
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => document.removeEventListener('visibilitychange', onVisible);
  });

  $effect(() => {
    if (active) {
      requestAnimationFrame(() => {
        refit();
        if (untrack(() => pane.state) === 'auth') firstInput?.focus();
        else term?.focus();
      });
    }
  });
</script>

<div class="pane" role="application" aria-label="终端 {host.name}">
  <div class="term" bind:this={container}></div>

  {#if pane.state === 'auth' || pane.state === 'error' || pane.state === 'closed'}
    <div class="overlay">
      <form class="card" onsubmit={submit}>
        <div class="title">
          <span class="mono">{target()}</span>
          {#if pane.state === 'error'}<span class="hint err">连接失败</span>{/if}
          {#if pane.state === 'closed'}<span class="hint">连接已关闭</span>{/if}
        </div>
        {#if pane.state === 'error' && pane.message}
          <p class="msg">{pane.message}</p>
        {/if}
        {#if ask === 'password'}
          <label class="field">
            <span>密码</span>
            <input class="input" type="password" autocomplete="current-password" bind:value={password} bind:this={firstInput} />
          </label>
          <label class="check"><input type="checkbox" bind:checked={savePassword} />保存密码到 Vault（加密）</label>
        {:else if ask === 'passphrase'}
          <label class="field">
            <span>私钥口令</span>
            <input class="input" type="password" bind:value={passphrase} bind:this={firstInput} />
          </label>
        {/if}
        <div class="row">
          {#if keyChanged}
            <button class="btn danger" type="button" onclick={trustNewKey}>我确认服务器已更换密钥</button>
          {/if}
          <button class="btn" type="submit" disabled={ask === 'password' && !password}>
            {pane.state === 'auth' ? '连接' : '重新连接'}
          </button>
        </div>
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
    width: min(380px, calc(100% - 32px));
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
  }
  .hint {
    font-size: 12px;
    color: var(--mut);
  }
  .hint.err {
    color: var(--err);
  }
  .msg {
    margin: 0;
    font-size: 12.5px;
    color: var(--mut);
    line-height: 1.6;
    user-select: text;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--mut);
  }
  .row {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  .row .btn {
    height: 34px;
  }
</style>
