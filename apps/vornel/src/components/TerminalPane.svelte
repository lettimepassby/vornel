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
  import { tick, untrack } from 'svelte';
  import * as api from '../lib/api';
  import type { Host } from '../lib/api';
  import { ensureFamily, familyStack, findPack } from '../lib/fonts';
  import { data, prefs } from '../lib/store.svelte';
  import { copyText } from '../lib/clipboard';

  const isMac = /Macintosh|Mac OS X|iPhone|iPad/.test(navigator.userAgent);
  import { badge, type BadgeKind } from '../lib/themes';
  import { resolveScheme } from '../lib/terminal-schemes';
  import { InputLine } from '../lib/inputline';
  import { forgetCommand, loadHistory, recordCommand, suggest, type Suggestion } from '../lib/suggest';
  import StatsBar from './StatsBar.svelte';
  import Icon from './Icon.svelte';

  let {
    host,
    active,
    onstatus,
    onhostkey,
    autoReconnect = false,
    onctrlused,
    visible = undefined,
    oninput,
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
    /** 是否显示在屏幕上（分屏时非焦点窗格也可见）；默认同 active */
    visible?: boolean;
    /** 键盘输入（分屏同步输入时转发给其他窗格） */
    oninput?: (data: string) => void;
  } = $props();

  let ctrlLatch = false;
  /** 测试接口的键：每个终端一个（同一台主机可以开多个会话） */
  const hookKey = `${untrack(() => host.id)}:${crypto.randomUUID()}`;

  /** 移动端按键栏的粘滞 Ctrl：下一个输入的字母会变成控制字符。 */
  export function setCtrl(on: boolean) {
    ctrlLatch = on;
    term?.focus();
  }

  let container = $state<HTMLDivElement>();
  let term: Terminal | undefined;
  let line: InputLine | undefined;
  let paneEl = $state<HTMLDivElement>();
  let fit: FitAddon | undefined;
  let webgl: WebglAddon | undefined;
  let sessionId: string | null = null;

  let pane = $state<PaneStatus>({ state: 'connecting' });
  /** 供资源栏使用的会话 id（sessionId 本身不是响应式的） */
  let sid = $state<string | null>(null);
  /** 服务器不是 Linux 时隐藏资源栏 */
  let statsOk = $state(true);
  const showStatsBar = $derived(host.protocol === 'ssh' && prefs.showStats && statsOk && pane.state === 'connected' && !!sid);
  let password = $state('');
  let savePassword = $state(false);
  let passphrase = $state('');
  let ask = $state<'password' | 'passphrase' | null>(null);
  /** 认证失败：弹出用户名 + 密码重新输入，成功后保存 */
  let authFailed = $state(false);
  let user = $state(untrack(() => host.username));
  let keyChanged = $state(false);
  let firstInput = $state<HTMLInputElement>();

  const terminalProtocols = ['ssh', 'mosh', 'telnet', 'serial'];
  /** 终端四周留白也用配色的背景色，避免换了配色后出现一圈界面底色 */
  const scheme = $derived(resolveScheme(prefs.terminalTheme, prefs.theme));
  const paneBg = $derived(scheme.theme.background);

  function setStatus(s: PaneStatus) {
    pane = s;
    sid = sessionId;
    onstatus({ ...s, cols: term?.cols, rows: term?.rows, sessionId });
  }

  function target() {
    if (host.protocol === 'serial') return host.address;
    return `${host.username ? host.username + '@' : ''}${host.address}:${host.port}`;
  }

  function line(kind: BadgeKind, label: string, text: string) {
    term?.write(badge(resolveScheme(prefs.terminalTheme, prefs.theme).dark ? 'ink' : 'paper', kind, label, text));
  }

  /** 写入会话但不抢焦点（分屏同步输入） */
  export function write(data: string) {
    line?.input(data);
    if (sessionId) void api.sessionWrite(sessionId, data).catch(() => {});
  }

  /** 把文字发送到终端（代码片段等）。 */
  export function send(text: string) {
    line?.input(text);
    if (sessionId) void api.sessionWrite(sessionId, text).catch(() => {});
    term?.focus();
  }

  /** 用户在这个终端里的输入：键盘、粘贴、接受联想 */
  function userInput(out: string) {
    line?.input(out);
    if (sessionId) void api.sessionWrite(sessionId, out).catch(() => {});
    oninput?.(out);
    if (out === '\r' || out === '\x03') sug = [];
    // 敲了字（不是方向键等）之后，之前按 Esc 关掉的联想重新开始
    if (!out.startsWith('\x1b') || out.startsWith('\x1b[200~')) sugHidden = false;
    scheduleSuggest();
  }

  // ---- 命令联想：输入时在光标下方列出以当前输入开头的命令
  let sug = $state<Suggestion[]>([]);
  let sugSel = $state(0);
  /** 联想所基于的输入（列表里这部分显示得淡一些） */
  let sugTyped = $state('');
  let sugPos = $state<{ left: number; top?: number; bottom?: number }>({ left: 0 });
  /** 按了 Esc 或刚接受了联想：直到下次输入前不再弹出 */
  let sugHidden = false;
  let sugFrame = 0;
  let termFont = $state('monospace');

  function scheduleSuggest() {
    if (!sugFrame) {
      sugFrame = requestAnimationFrame(() => {
        sugFrame = 0;
        updateSuggest();
      });
    }
  }

  function updateSuggest() {
    if (!term || !line || !prefs.commandSuggest || !active || sugHidden || pane.state !== 'connected') {
      if (sug.length) sug = [];
      return;
    }
    const r = line.read();
    if (r === 'pending') return; // 回显还没到，保持现状
    const b = term.buffer.active;
    if (!r || !r.atEnd || b.viewportY !== b.baseY) {
      if (sug.length) sug = [];
      return;
    }
    const list = suggest(r.text, host.id);
    if (!list.length) {
      if (sug.length) sug = [];
      return;
    }
    const prev = sug[sugSel]?.text;
    sug = list;
    sugTyped = r.text;
    sugSel = Math.max(0, list.findIndex((x) => x.text === prev));
    placeSuggest(r.startX, r.startY);
  }

  /** 列表放在输入开始处的下一行；靠近底部时放到上方 */
  function placeSuggest(startX: number, startY: number) {
    const screen = container?.querySelector('.xterm-screen');
    if (!term || !screen || !paneEl) return;
    const sr = screen.getBoundingClientRect();
    const pr = paneEl.getBoundingClientRect();
    const cw = sr.width / term.cols;
    const ch = sr.height / term.rows;
    const row = term.buffer.active.cursorY;
    const col = startY === row ? startX : 0;
    const left = Math.max(8, Math.min(sr.left - pr.left + col * cw - 6, pr.width - 340));
    const below = sr.top - pr.top + (row + 1) * ch + 4;
    const spaceBelow = pr.height - below;
    // 下方放不下时放到输入行（可能自动换行占了几行）的上方
    const topRow = Math.max(0, startY);
    if (spaceBelow < 200 && topRow * ch > spaceBelow) {
      sugPos = { left, bottom: pr.bottom - (sr.top + topRow * ch) + 4 };
    } else {
      sugPos = { left, top: below };
    }
  }

  function acceptSuggestion(i: number) {
    const s = sug[i];
    if (!s || !line) return;
    // 键入的文字可能比屏幕回显更新：只要建议仍以它开头，补上剩下的部分
    const cur = line.exact ? line.typed : sugTyped;
    sug = [];
    if (!s.text.startsWith(cur)) return;
    const rest = s.text.slice(cur.length);
    if (rest) userInput(rest);
    sugHidden = true;
    term?.focus();
  }

  function forgetSuggestion(i: number) {
    const s = sug[i];
    if (!s || s.source !== 'history') return;
    forgetCommand(s.text);
    sug = sug.filter((_, j) => j !== i);
    sugSel = Math.min(sugSel, Math.max(0, sug.length - 1));
    scheduleSuggest();
  }

  /** 联想列表打开时接管 ↑ ↓ → Esc（以及 Shift+Delete 删除历史）；返回 true 表示已处理 */
  function suggestKey(e: KeyboardEvent) {
    if (!sug.length || e.type !== 'keydown' || e.isComposing || e.altKey || e.ctrlKey || e.metaKey) return false;
    if (e.key === 'ArrowDown' && !e.shiftKey) sugSel = (sugSel + 1) % sug.length;
    else if (e.key === 'ArrowUp' && !e.shiftKey) sugSel = (sugSel - 1 + sug.length) % sug.length;
    else if (e.key === 'ArrowRight' && !e.shiftKey) acceptSuggestion(sugSel);
    else if (e.key === 'Escape') {
      sug = [];
      sugHidden = true;
    } else if (e.key === 'Delete' && e.shiftKey && sug[sugSel]?.source === 'history') forgetSuggestion(sugSel);
    else return false;
    e.preventDefault();
    return true;
  }

  // ---- 右键菜单用到的终端操作
  export function selection() {
    return term?.hasSelection() ? term.getSelection() : '';
  }
  /** 粘贴：走 xterm 的 paste，远端开启 bracketed paste 时会正确包裹 */
  export function paste(text: string) {
    if (text) term?.paste(text);
    term?.focus();
  }
  export function selectAll() {
    term?.selectAll();
  }
  export function clear() {
    term?.clear();
    term?.focus();
  }
  export function canReconnect() {
    return pane.state === 'closed' || pane.state === 'error';
  }
  export function reconnect() {
    if (canReconnect()) void connect();
  }

  async function connect() {
    if (!term) return;
    keyChanged = false;
    setStatus({ state: 'connecting' });
    line('info', '···', `正在连接 ${target()}${host.jumpHost ? '（经由跳板机）' : ''}`);
    try {
      if (authFailed) {
        const name = user.trim();
        const switchToPassword = host.auth.type !== 'password' && !!password;
        if ((name && name !== host.username) || switchToPassword) {
          await api.save('host', {
            ...$state.snapshot(host),
            username: name || host.username,
            ...(switchToPassword ? { auth: { type: 'password' } } : {}),
          });
          onhostkey?.(); // 刷新主机列表
        }
      }
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
          } else if (ev.type === 'os') {
            onhostkey?.(); // 主机记录更新了系统类型，刷新列表图标
          } else if (ev.type === 'connected') {
            authFailed = false;
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
      // “已连接”事件会先于 sessionOpen 返回到达，那时还拿不到会话 id：这里补上（资源栏依赖它）
      sid = sessionId;
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
      // 认证失败（保存的密码错了、换过密码、用户名不对等）：弹出输入框，连接成功后保存
      if (message.includes('认证失败') && (host.protocol === 'ssh' || host.protocol === 'mosh')) {
        line('err', 'ERR', message);
        authFailed = true;
        ask = 'password';
        password = '';
        savePassword = true;
        user = host.username;
        setStatus({ state: 'auth', message });
        void tick().then(() => firstInput?.focus());
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
    termFont = term.options.fontFamily ?? 'monospace';
    term.options.fontSize = prefs.terminalFontSize;
    term.options.lineHeight = prefs.terminalLineHeight ?? pack.lineHeight;
    term.options.theme = resolveScheme(prefs.terminalTheme, prefs.theme).theme;
    term.options.cursorStyle = prefs.cursorStyle;
    term.options.cursorBlink = prefs.cursorBlink;
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
        lineHeight: untrack(() => prefs.terminalLineHeight) ?? pack.lineHeight,
        theme: untrack(() => resolveScheme(prefs.terminalTheme, prefs.theme).theme),
        cursorStyle: untrack(() => prefs.cursorStyle),
        cursorBlink: untrack(() => prefs.cursorBlink),
        scrollback: 10000,
        macOptionIsMeta: true,
        screenReaderMode: untrack(() => prefs.screenReader),
      });
      termFont = term.options.fontFamily ?? 'monospace';
      line = new InputLine(term, (cmd) => {
        if (prefs.commandSuggest && prefs.commandHistory) recordCommand(host.id, cmd);
      });
      if (untrack(() => prefs.commandSuggest)) void loadHistory();
      term.onWriteParsed(() => {
        line?.flush();
        if (sug.length || line?.tracking) scheduleSuggest();
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
      // 只读的测试接口：端到端测试用它读出终端缓冲区（WebGL 渲染时 DOM 里没有文字）
      const w = window as unknown as { __vornelTerminals?: Record<string, () => string> };
      w.__vornelTerminals ??= {};
      w.__vornelTerminals[hookKey] = () => {
        const b = term!.buffer.active;
        const lines: string[] = [];
        for (let i = 0; i < b.length; i++) lines.push(b.getLine(i)?.translateToString(true) ?? '');
        return lines.join('\n');
      };
      // 复制：选中即复制（可在设置里关闭），以及 ⌘C / Ctrl+Shift+C
      el.addEventListener('mouseup', () => {
        if (prefs.copyOnSelect && term?.hasSelection()) void copyText(term.getSelection());
      });
      term.attachCustomKeyEventHandler((e) => {
        if (suggestKey(e)) return false;
        const isCopy = e.type === 'keydown' && e.key.toLowerCase() === 'c' && (isMac ? e.metaKey : e.ctrlKey && e.shiftKey);
        if (isCopy && term?.hasSelection()) {
          void copyText(term.getSelection());
          return false;
        }
        return true;
      });
      term.onData((d) => {
        let out = d;
        if (ctrlLatch && d.length === 1) {
          const c = d.toUpperCase().charCodeAt(0);
          if (c >= 64 && c <= 95) out = String.fromCharCode(c - 64);
          ctrlLatch = false;
          onctrlused?.();
        }
        userInput(out);
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
      delete (window as unknown as { __vornelTerminals?: Record<string, () => string> }).__vornelTerminals?.[hookKey];
      ro.disconnect();
      cancelAnimationFrame(sugFrame);
      line?.dispose();
      line = undefined;
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
    void prefs.terminalTheme;
    void prefs.terminalLineHeight;
    void prefs.cursorStyle;
    void prefs.cursorBlink;
    void applyAppearance();
  });

  // 切走、关掉联想、断开连接时收起列表；打开联想时预读命令历史
  $effect(() => {
    if (!active || !prefs.commandSuggest || pane.state !== 'connected') {
      sug = [];
      sugHidden = false;
    }
    if (prefs.commandSuggest) void loadHistory();
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

<div class="pane" role="application" aria-label="终端 {host.name}" style:background={paneBg} bind:this={paneEl}>
  <div class="term" bind:this={container} style:bottom={showStatsBar ? '30px' : '6px'}></div>
  {#if sug.length}
    <div
      class="suggest"
      role="listbox"
      aria-label="命令联想"
      style:left="{sugPos.left}px"
      style:top={sugPos.top != null ? `${sugPos.top}px` : undefined}
      style:bottom={sugPos.bottom != null ? `${sugPos.bottom}px` : undefined}
      style:--sg-bg={scheme.theme.background}
      style:--sg-fg={scheme.theme.foreground}
      style:--sg-acc={scheme.theme.cursor ?? scheme.theme.blue}
      style:font-family={termFont}
      style:font-size="{Math.max(11, prefs.terminalFontSize - 1)}px"
    >
      {#each sug as s, i (s.text)}
        <div
          class="sg"
          class:on={i === sugSel}
          role="option"
          tabindex="-1"
          aria-selected={i === sugSel}
          onmousedown={(e) => {
            e.preventDefault();
            acceptSuggestion(i);
          }}
          onmouseenter={() => (sugSel = i)}
        >
          <span class="src" title={s.source === 'history' ? '执行过的命令' : s.source === 'snippet' ? '代码片段' : '常用命令'}>
            <Icon name={s.source === 'history' ? 'clock' : s.source === 'snippet' ? 'code' : 'terminal'} size={12} />
          </span>
          <span class="cmd"><span class="typed">{sugTyped}</span>{s.text.slice(sugTyped.length)}</span>
          {#if s.label}<span class="lbl">{s.label}</span>{/if}
          {#if s.source === 'history'}
            <button
              class="forget"
              type="button"
              tabindex="-1"
              title="从历史中删除（Shift+Delete）"
              aria-label="从历史中删除"
              onmousedown={(e) => {
                e.preventDefault();
                e.stopPropagation();
                forgetSuggestion(i);
              }}><Icon name="close" size={11} /></button
            >
          {/if}
        </div>
      {/each}
      <div class="foot"><kbd>→</kbd>补全<kbd>↑↓</kbd>选择<kbd>Esc</kbd>关闭</div>
    </div>
  {/if}
  {#if showStatsBar && sid}
    <div class="stats-wrap" style:color={scheme.theme.foreground}>
      <StatsBar sessionId={sid} active={visible ?? active} onunsupported={() => (statsOk = false)} />
    </div>
  {/if}

  {#if pane.state === 'auth' || pane.state === 'error' || pane.state === 'closed'}
    <div class="overlay">
      <form class="card" class:shake={authFailed} onsubmit={submit}>
        <div class="title">
          <span class="mono">{target()}</span>
          {#if authFailed}<span class="hint err">认证失败</span>{/if}
          {#if pane.state === 'error'}<span class="hint err">连接失败</span>{/if}
          {#if pane.state === 'closed'}<span class="hint">连接已关闭</span>{/if}
        </div>
        {#if pane.state === 'error' && pane.message}
          <p class="msg">{pane.message}</p>
        {/if}
        {#if authFailed}
          <p class="msg">用户名、密码或私钥不正确。请重新输入，连接成功后会保存到这台主机。</p>
          <label class="field">
            <span>用户名</span>
            <input class="input mono" autocomplete="username" bind:value={user} />
          </label>
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
  /* 命令联想：颜色取自终端配色，和终端融为一体 */
  .suggest {
    position: absolute;
    /* 高于 xterm 的无障碍层与滚动条（10–11），低于连接遮罩（50） */
    z-index: 40;
    min-width: 220px;
    max-width: min(560px, calc(100% - 16px));
    padding: 4px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--sg-bg) 90%, var(--sg-fg));
    color: var(--sg-fg);
    border: 1px solid color-mix(in srgb, var(--sg-fg) 16%, transparent);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.28);
    animation: sg-in 110ms cubic-bezier(0.2, 0.8, 0.2, 1);
    user-select: none;
  }
  @keyframes sg-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
  }
  .sg {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 1.9em;
    padding: 0 6px 0 7px;
    border-radius: 5px;
    cursor: pointer;
  }
  .sg.on {
    background: color-mix(in srgb, var(--sg-acc) 22%, transparent);
  }
  .src {
    display: grid;
    place-items: center;
    flex: none;
    opacity: 0.55;
  }
  .sg.on .src {
    color: var(--sg-acc);
    opacity: 1;
  }
  .cmd {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: pre;
  }
  .cmd .typed {
    opacity: 0.5;
  }
  .lbl {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--f-ui);
    font-size: 11px;
    opacity: 0.55;
  }
  .forget {
    display: none;
    place-items: center;
    flex: none;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: inherit;
    opacity: 0.6;
    cursor: pointer;
  }
  .sg:hover .forget {
    display: grid;
  }
  .forget:hover {
    opacity: 1;
    background: color-mix(in srgb, var(--sg-fg) 14%, transparent);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 7px 2px;
    margin-top: 3px;
    border-top: 1px solid color-mix(in srgb, var(--sg-fg) 10%, transparent);
    font-family: var(--f-ui);
    font-size: 10.5px;
    opacity: 0.6;
  }
  .foot kbd {
    margin-left: 6px;
    padding: 0 4px;
    border-radius: 3px;
    font: inherit;
    background: color-mix(in srgb, var(--sg-fg) 12%, transparent);
  }
  .foot kbd:first-child {
    margin-left: 0;
  }

  /* 认证失败时轻轻摇一下，提示需要重新输入 */
  .card.shake {
    animation: shake 360ms cubic-bezier(0.36, 0.07, 0.19, 0.97);
  }
  @keyframes shake {
    20%,
    60% {
      transform: translateX(-6px);
    }
    40%,
    80% {
      transform: translateX(6px);
    }
  }
  .pane {
    position: absolute;
    inset: 0;
    display: flex;
    background: var(--pane);
  }
  /* 留白用绝对定位实现，不放在容器的 padding 上：FitAddon 按父容器的高度计算行数，
     border-box 下 padding 会被算进可用高度，导致多算一行、最后一行被切掉 */
  .term {
    position: absolute;
    top: 12px;
    right: 6px;
    bottom: 6px;
    left: 14px;
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
    /* xterm.js 的无障碍层与滚动条 z-index 为 10–11 且铺满终端，必须高于它们，否则点击和键盘焦点会被截走 */
    z-index: 50;
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
