<script lang="ts">
  // 会话右侧的“外观”面板：快速调整终端配色、字体、字号、行高、光标（改动立即生效，并随偏好同步）
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import * as api from '../lib/api';
  import { FONT_PACKS, findPack } from '../lib/fonts';
  import { prefs } from '../lib/store.svelte';
  import { SCHEMES, resolveScheme } from '../lib/terminal-schemes';
  import Icon from './Icon.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const pack = $derived(findPack(prefs.fontPack));
  const auto = $derived(resolveScheme('auto', prefs.theme).theme);
  const builtinMono = [...new Set(FONT_PACKS.flatMap((p) => [p.terminal, p.mono]))].sort();
  let systemMono = $state<string[]>([]);
  $effect(() => {
    void api
      .systemFonts()
      .then((f) => (systemMono = f.filter((x) => x.monospace).map((x) => x.name).filter((n) => !builtinMono.includes(n))))
      .catch(() => {});
  });

  function setFont(name: string) {
    const next = { ...prefs.fontOverrides };
    if (name) next.terminal = name;
    else delete next.terminal;
    prefs.fontOverrides = next;
  }
  const size = (d: number) => (prefs.terminalFontSize = Math.min(24, Math.max(10, prefs.terminalFontSize + d)));
  function reset() {
    prefs.terminalTheme = 'auto';
    prefs.terminalLineHeight = null;
    prefs.cursorStyle = 'block';
    prefs.cursorBlink = true;
    prefs.terminalFontSize = 13;
    setFont('');
  }
  const swatch = (t: { background?: string; red?: string; green?: string; yellow?: string; blue?: string; magenta?: string; cyan?: string }) =>
    [t.red, t.green, t.yellow, t.blue, t.magenta, t.cyan];
</script>

<aside class="ap" aria-label="终端外观" transition:fly={{ x: 24, duration: 180, easing: cubicOut }}>
  <header>
    <b>终端外观</b>
    <button class="icon-btn" type="button" aria-label="关闭外观面板" onclick={onclose}><Icon name="close" size={14} /></button>
  </header>

  <div class="body">
    <section>
      <h3>配色</h3>
      <div class="schemes" role="radiogroup" aria-label="终端配色">
        <button type="button" class="sc" role="radio" aria-checked={prefs.terminalTheme === 'auto'} aria-label="跟随界面" onclick={() => (prefs.terminalTheme = 'auto')}>
          <span class="pv" style:background={auto.background} style:color={auto.foreground}>
            Aa{#each swatch(auto) as c, i (i)}<i style:background={c}></i>{/each}
          </span>
          <span class="nm">跟随界面</span>
        </button>
        {#each SCHEMES as s (s.id)}
          <button type="button" class="sc" role="radio" aria-checked={prefs.terminalTheme === s.id} aria-label={s.name} onclick={() => (prefs.terminalTheme = s.id)}>
            <span class="pv" style:background={s.theme.background} style:color={s.theme.foreground}>
              Aa{#each swatch(s.theme) as c, i (i)}<i style:background={c}></i>{/each}
            </span>
            <span class="nm">{s.name}</span>
          </button>
        {/each}
      </div>
    </section>

    <section>
      <h3>字体</h3>
      <select class="input" aria-label="终端字体" value={prefs.fontOverrides.terminal ?? ''} onchange={(e) => setFont(e.currentTarget.value)}>
        <option value="">跟随字体包（{pack.terminal}）</option>
        <optgroup label="内置">
          {#each builtinMono as f (f)}<option value={f}>{f}</option>{/each}
        </optgroup>
        {#if systemMono.length}
          <optgroup label="本机已安装">
            {#each systemMono as f (f)}<option value={f}>{f}</option>{/each}
          </optgroup>
        {/if}
      </select>
    </section>

    <section>
      <h3>字号 <span class="val mono">{prefs.terminalFontSize}</span></h3>
      <div class="row">
        <button class="icon-btn" type="button" aria-label="减小字号" onclick={() => size(-1)}>−</button>
        <input type="range" min="10" max="24" step="1" aria-label="字号" bind:value={prefs.terminalFontSize} />
        <button class="icon-btn" type="button" aria-label="增大字号" onclick={() => size(1)}>+</button>
      </div>
    </section>

    <section>
      <h3>
        行高 <span class="val mono">{(prefs.terminalLineHeight ?? pack.lineHeight).toFixed(2)}</span>
        {#if prefs.terminalLineHeight !== null}<button class="link" type="button" onclick={() => (prefs.terminalLineHeight = null)}>默认</button>{/if}
      </h3>
      <input
        type="range"
        min="1"
        max="1.8"
        step="0.05"
        aria-label="行高"
        value={prefs.terminalLineHeight ?? pack.lineHeight}
        oninput={(e) => (prefs.terminalLineHeight = Number(e.currentTarget.value))}
      />
    </section>

    <section>
      <h3>光标</h3>
      <div class="seg" role="group" aria-label="光标样式">
        <button type="button" aria-pressed={prefs.cursorStyle === 'block'} onclick={() => (prefs.cursorStyle = 'block')}>方块</button>
        <button type="button" aria-pressed={prefs.cursorStyle === 'bar'} onclick={() => (prefs.cursorStyle = 'bar')}>竖线</button>
        <button type="button" aria-pressed={prefs.cursorStyle === 'underline'} onclick={() => (prefs.cursorStyle = 'underline')}>下划线</button>
      </div>
      <label class="check"><input type="checkbox" bind:checked={prefs.cursorBlink} />光标闪烁</label>
    </section>

    <section>
      <h3>资源栏</h3>
      <label class="check"><input type="checkbox" bind:checked={prefs.showStats} />SSH 终端底部显示服务器资源</label>
      <p class="hint">经已有连接每 3 秒读取一次 /proc，只在会话可见时读取，对服务器几乎没有负担。仅支持 Linux。</p>
    </section>

    <button class="btn ghost reset" type="button" onclick={reset}><Icon name="refresh" size={14} />恢复默认</button>
  </div>
</aside>

<style>
  .ap {
    width: 280px;
    flex: none;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--line);
    background: var(--win);
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 42px;
    padding: 0 8px 0 16px;
    border-bottom: 1px solid var(--line);
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding: 6px 16px 20px;
  }
  section {
    margin-top: 14px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--mut);
  }
  .val {
    color: var(--fg);
    font-weight: 500;
  }
  .schemes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .sc {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .pv {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 34px;
    padding: 0 8px;
    border-radius: 7px;
    box-shadow: 0 0 0 1px var(--line2);
    font: 600 12px var(--f-mono);
    transition:
      box-shadow 120ms ease-out,
      transform 120ms ease-out;
  }
  .pv i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .pv i:first-of-type {
    margin-left: auto;
  }
  .sc:hover .pv {
    transform: translateY(-1px);
  }
  .sc[aria-checked='true'] .pv {
    box-shadow: 0 0 0 2px var(--acc);
  }
  .nm {
    font-size: 11.5px;
    color: var(--mut);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sc[aria-checked='true'] .nm {
    color: var(--fg);
  }
  select {
    width: 100%;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  input[type='range'] {
    flex: 1;
    width: 100%;
    accent-color: var(--acc);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    font-size: 13px;
  }
  .hint {
    margin: 6px 0 0;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--mut);
  }
  .reset {
    margin-top: 22px;
    width: 100%;
    justify-content: center;
  }
</style>
