<script lang="ts">
  // RDP / VNC 远程桌面：后端只推送变化的矩形区域（脏矩形），这里直接 putImageData 到 canvas。
  import { untrack } from 'svelte';
  import * as api from '../lib/api';
  import type { Host } from '../lib/api';
  import { remoteClose, remoteInput, remoteOpen, type RemoteInput } from '../lib/remote';
  import type { PaneStatus } from './TerminalPane.svelte';

  let { host, active, onstatus }: { host: Host; active: boolean; onstatus: (s: PaneStatus) => void } = $props();

  let canvas = $state<HTMLCanvasElement>();
  let wrap = $state<HTMLDivElement>();
  let ctx: CanvasRenderingContext2D | null = null;
  let sessionId: string | null = null;
  let phase = $state<PaneStatus['state']>('connecting');
  let message = $state('');
  let password = $state('');
  let savePassword = $state(false);
  let size = $state({ w: 0, h: 0 });

  function setState(s: PaneStatus['state'], msg = '') {
    phase = s;
    message = msg;
    onstatus({ state: s, message: msg, sessionId, cols: size.w || undefined, rows: size.h || undefined });
  }

  function drawRect(buf: ArrayBuffer) {
    if (!ctx || buf.byteLength < 8) return;
    const v = new DataView(buf);
    const x = v.getUint16(0, true);
    const y = v.getUint16(2, true);
    const w = v.getUint16(4, true);
    const h = v.getUint16(6, true);
    if (!w || !h || buf.byteLength < 8 + w * h * 4) return;
    ctx.putImageData(new ImageData(new Uint8ClampedArray(buf, 8, w * h * 4), w, h), x, y);
  }

  async function connect() {
    if (!wrap || !canvas) return;
    setState('connecting');
    const rect = wrap.getBoundingClientRect();
    const scale = window.devicePixelRatio > 1 ? 1 : 1;
    const width = Math.max(800, Math.floor(rect.width * scale) & ~1);
    const height = Math.max(600, Math.floor(rect.height * scale) & ~1);
    try {
      sessionId = await remoteOpen({
        hostId: host.id,
        password: password || undefined,
        savePassword,
        width,
        height,
        onFrame: drawRect,
        onEvent: (ev) => {
          if (ev.type === 'resize') {
            size = { w: ev.width, h: ev.height };
            if (canvas) {
              canvas.width = ev.width;
              canvas.height = ev.height;
              ctx = canvas.getContext('2d', { alpha: false });
            }
          } else if (ev.type === 'connected') {
            setState('connected');
            canvas?.focus();
          } else if (ev.type === 'closed') {
            sessionId = null;
            setState('closed', ev.reason ?? '');
          }
        },
      });
      password = '';
    } catch (e) {
      sessionId = null;
      const m = api.errorText(e);
      if (m.startsWith('NEED_PASSWORD')) setState('auth');
      else setState('error', m);
    }
  }

  function send(ev: RemoteInput) {
    if (sessionId) void remoteInput(sessionId, ev).catch(() => {});
  }

  function pointer(e: PointerEvent | WheelEvent, kind: 'move' | 'down' | 'up' | 'wheel') {
    if (!canvas || !size.w) return;
    const r = canvas.getBoundingClientRect();
    const x = Math.round(((e.clientX - r.left) / r.width) * size.w);
    const y = Math.round(((e.clientY - r.top) / r.height) * size.h);
    if (kind === 'wheel') {
      const we = e as WheelEvent;
      send({ type: 'wheel', x, y, delta: Math.sign(-we.deltaY) * 120 });
    } else {
      send({ type: 'pointer', x, y, button: (e as PointerEvent).button, down: kind === 'down', move: kind === 'move' });
    }
  }

  function key(e: KeyboardEvent, down: boolean) {
    e.preventDefault();
    send({ type: 'key', code: e.code, key: e.key, down });
  }

  $effect(() => {
    if (!canvas || !wrap) return;
    const h = untrack(() => host);
    if (!h.savePassword) setState('auth');
    else void connect();
    return () => {
      if (sessionId) void remoteClose(sessionId).catch(() => {});
      sessionId = null;
    };
  });

  $effect(() => {
    if (active && phase === 'connected') requestAnimationFrame(() => canvas?.focus());
  });
</script>

<div class="rd" bind:this={wrap}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <canvas
    bind:this={canvas}
    tabindex="0"
    aria-label="远程桌面 {host.name}"
    onpointermove={(e) => pointer(e, 'move')}
    onpointerdown={(e) => {
      canvas?.focus();
      pointer(e, 'down');
    }}
    onpointerup={(e) => pointer(e, 'up')}
    onwheel={(e) => {
      e.preventDefault();
      pointer(e, 'wheel');
    }}
    onkeydown={(e) => key(e, true)}
    onkeyup={(e) => key(e, false)}
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>

  {#if phase !== 'connected'}
    <div class="overlay">
      <form
        class="card"
        onsubmit={(e) => {
          e.preventDefault();
          void connect();
        }}
      >
        <div class="title">
          <span class="mono">{host.protocol.toUpperCase()} · {host.address}:{host.port}</span>
          {#if phase === 'connecting'}<span class="hint">正在连接…</span>{/if}
        </div>
        {#if message}<p class="msg">{message}</p>{/if}
        {#if phase === 'auth' || phase === 'error' || phase === 'closed'}
          {#if phase === 'auth' || !host.savePassword}
            <label class="field">
              <span>密码</span>
              <!-- svelte-ignore a11y_autofocus -->
              <input class="input" type="password" bind:value={password} autofocus />
            </label>
            <label class="check"><input type="checkbox" bind:checked={savePassword} />保存密码到 Vault（加密）</label>
          {/if}
          <button class="btn" type="submit">{phase === 'auth' ? '连接' : '重新连接'}</button>
        {/if}
      </form>
    </div>
  {/if}
</div>

<style>
  .rd {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: #000;
    overflow: hidden;
  }
  canvas {
    max-width: 100%;
    max-height: 100%;
    outline: none;
    cursor: default;
    image-rendering: auto;
  }
  .overlay {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--pane) 85%, transparent);
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
    justify-content: space-between;
    gap: 8px;
  }
  .hint {
    font-size: 12px;
    color: var(--mut);
  }
  .msg {
    margin: 0;
    font-size: 12.5px;
    color: var(--err);
    user-select: text;
  }
  .card .btn {
    height: 34px;
  }
</style>
