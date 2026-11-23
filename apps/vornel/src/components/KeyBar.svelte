<script lang="ts">
  // 移动端终端按键栏（DESIGN.md 5.4）：系统键盘弹出时贴在键盘上方。
  let { onkey, ctrl, onctrl }: { onkey: (seq: string) => void; ctrl: boolean; onctrl: (on: boolean) => void } = $props();

  const KEYS: { label: string; seq: string; aria: string }[] = [
    { label: 'esc', seq: '\x1b', aria: 'Esc' },
    { label: 'tab', seq: '\t', aria: 'Tab' },
    { label: '↑', seq: '\x1b[A', aria: '上' },
    { label: '↓', seq: '\x1b[B', aria: '下' },
    { label: '←', seq: '\x1b[D', aria: '左' },
    { label: '→', seq: '\x1b[C', aria: '右' },
    { label: '|', seq: '|', aria: '竖线' },
    { label: '/', seq: '/', aria: '斜杠' },
    { label: '~', seq: '~', aria: '波浪号' },
    { label: '-', seq: '-', aria: '减号' },
  ];
</script>

<div class="keys" role="toolbar" aria-label="终端按键">
  <button type="button" class:on={ctrl} aria-pressed={ctrl} onclick={() => onctrl(!ctrl)}>ctrl</button>
  {#each KEYS as k (k.label)}
    <button type="button" aria-label={k.aria} onclick={() => onkey(k.seq)}>{k.label}</button>
  {/each}
</div>

<style>
  .keys {
    display: flex;
    gap: 6px;
    padding: 8px 8px calc(8px + env(safe-area-inset-bottom, 0px));
    background: var(--sb);
    border-top: 1px solid var(--line);
    overflow-x: auto;
    scrollbar-width: none;
  }
  button {
    flex: none;
    min-width: 44px;
    height: 40px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: var(--raise);
    color: var(--fg);
    font: 500 14px var(--f-mono);
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.3);
  }
  button.on {
    background: var(--acc);
    color: var(--on-acc);
  }
</style>
