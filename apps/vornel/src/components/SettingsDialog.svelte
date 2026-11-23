<script lang="ts">
  import { ensureFamily, familyStack, FONT_PACKS } from '../lib/fonts';
  import { settings } from '../lib/settings.svelte';
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';

  let { onclose }: { onclose: () => void } = $props();

  // 打开设置时预加载所有字体包，让预览卡片显示真实字体
  $effect(() => {
    for (const p of FONT_PACKS) {
      void ensureFamily(p.ui);
      void ensureFamily(p.terminal);
    }
  });

  function setSize(delta: number) {
    settings.terminalFontSize = Math.min(24, Math.max(10, settings.terminalFontSize + delta));
  }
</script>

<Dialog title="外观" width={640} {onclose}>
  <section>
    <div class="row">
      <div><b>主题</b></div>
      <div class="seg" role="group" aria-label="主题">
        <button type="button" aria-pressed={settings.theme === 'ink'} onclick={() => (settings.theme = 'ink')}>深色 Ink</button>
        <button type="button" aria-pressed={settings.theme === 'paper'} onclick={() => (settings.theme = 'paper')}>浅色 Paper</button>
      </div>
    </div>

    <div class="block">
      <b>字体包</b>
      <span class="sub">英文字体已内置、离线可用；界面中文使用系统字体。中文增强包将在后续版本提供下载。</span>
      <div class="packs">
        {#each FONT_PACKS as p (p.id)}
          <button
            type="button"
            class="pk"
            class:on={settings.fontPack === p.id}
            aria-pressed={settings.fontPack === p.id}
            onclick={() => (settings.fontPack = p.id)}
          >
            <span class="pv">
              <span class="a" style:font-family={familyStack(p.ui, 'sans-serif')}>Aa 主机</span>
              <span class="t" style:font-family={familyStack(p.terminal, 'monospace')}>❯ ssh web-01</span>
            </span>
            <span class="nm">{p.name}<span class="chk"><Icon name="chevron" size={12} /></span></span>
            <span class="fs">{p.ui} · {p.terminal}</span>
          </button>
        {/each}
      </div>
    </div>

    <div class="row">
      <div><b>终端字号</b></div>
      <div class="stepper">
        <button class="icon-btn" type="button" aria-label="减小字号" onclick={() => setSize(-1)}>−</button>
        <span class="mono">{settings.terminalFontSize}</span>
        <button class="icon-btn" type="button" aria-label="增大字号" onclick={() => setSize(1)}>+</button>
      </div>
    </div>
  </section>
</Dialog>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  b {
    font-weight: 600;
    font-size: 13.5px;
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .sub {
    font-size: 12px;
    color: var(--mut);
  }
  .packs {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    margin-top: 10px;
  }
  .pk {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 8px;
    border: 1px solid var(--line2);
    border-radius: 11px;
    background: var(--win);
    text-align: left;
    transition: border-color 120ms ease-out;
  }
  .pk:hover {
    border-color: color-mix(in srgb, var(--acc) 50%, var(--line2));
  }
  .pk.on {
    border-color: var(--acc);
    box-shadow: 0 0 0 1px var(--acc);
  }
  .pv {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    height: 58px;
    padding: 0 10px;
    border-radius: 7px;
    background: var(--pane);
    overflow: hidden;
  }
  .a {
    font-size: 18px;
    font-weight: 600;
  }
  .t {
    font-size: 11.5px;
    color: var(--ok);
    white-space: nowrap;
  }
  .nm {
    display: flex;
    align-items: center;
    padding: 0 2px;
    font-weight: 600;
  }
  .chk {
    margin-left: auto;
    color: var(--acc);
    opacity: 0;
  }
  .pk.on .chk {
    opacity: 1;
  }
  .fs {
    padding: 0 2px 2px;
    font: 11px var(--f-mono);
    color: var(--mut);
  }
  .stepper {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .stepper .mono {
    min-width: 24px;
    text-align: center;
  }
</style>
