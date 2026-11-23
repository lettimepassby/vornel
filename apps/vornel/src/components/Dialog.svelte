<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    title,
    width = 440,
    onclose,
    children,
  }: { title: string; width?: number; onclose: () => void; children: Snippet } = $props();

  let dialog = $state<HTMLDialogElement>();

  // 用原生 <dialog>：自带焦点锁定和 Esc 关闭
  $effect(() => {
    dialog?.showModal();
  });
</script>

<dialog
  bind:this={dialog}
  style:width="{width}px"
  aria-label={title}
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
  onclick={(e) => e.target === dialog && onclose()}
>
  <header>
    <h2>{title}</h2>
    <button class="icon-btn" type="button" aria-label="关闭" onclick={onclose}><Icon name="close" /></button>
  </header>
  <div class="body">
    {@render children()}
  </div>
</dialog>

<style>
  dialog {
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px);
    padding: 0;
    border: 0;
    border-radius: 14px;
    background: var(--raise);
    color: var(--fg);
    box-shadow: var(--shadow);
    overflow: auto;
    animation: in 200ms var(--ease);
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 14px 4px 20px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .body {
    padding: 12px 20px 20px;
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }
</style>
