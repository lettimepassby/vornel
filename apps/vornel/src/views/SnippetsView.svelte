<script lang="ts">
  import * as api from '../lib/api';
  import type { Snippet } from '../lib/api';
  import { data, loadAll } from '../lib/store.svelte';
  import Dialog from '../components/Dialog.svelte';
  import Icon from '../components/Icon.svelte';

  /** 当前有终端会话时，可以直接发送到它 */
  let { onsend }: { onsend: ((s: Snippet) => void) | null } = $props();

  let editing = $state<{ s: Snippet; isNew: boolean } | null>(null);
  let error = $state('');
  let confirmId = $state<string | null>(null);

  function edit(s?: Snippet) {
    error = '';
    editing = s
      ? { s: { ...s }, isNew: false }
      : { s: { id: crypto.randomUUID(), name: '', command: '', run: true }, isNew: true };
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!editing) return;
    try {
      await api.save('snippet', editing.s);
      editing = null;
      await loadAll();
    } catch (err) {
      error = api.errorText(err);
    }
  }

  async function del(s: Snippet) {
    if (confirmId !== s.id) {
      confirmId = s.id;
      return;
    }
    await api.remove('snippet', s.id);
    confirmId = null;
    await loadAll();
  }
</script>

<div class="page">
  <div class="page-head">
    <h2>代码片段</h2>
    <span class="n">{data.snippets.length}</span>
    <span class="sp"></span>
    <button class="btn" type="button" onclick={() => edit()}><Icon name="plus" size={14} stroke={2} />新建片段</button>
  </div>
  <p class="page-sub">常用命令一键发送到当前终端。在终端标签上方的“片段”按钮也能快速调用。</p>

  {#if data.snippets.length}
    <div class="items">
      {#each data.snippets as s (s.id)}
        <div class="item">
          <div class="main">
            <b>{s.name} {#if s.run}<span class="tag">自动回车</span>{/if}</b>
            <small>{s.command}</small>
          </div>
          <div class="acts">
            {#if onsend}
              <button class="btn ghost" type="button" onclick={() => onsend?.(s)}><Icon name="send" size={14} />发送</button>
            {/if}
            <button class="icon-btn" type="button" aria-label="编辑 {s.name}" onclick={() => edit(s)}><Icon name="edit" size={15} /></button>
            <button class="btn danger" type="button" onclick={() => del(s)}>{confirmId === s.id ? '确认删除？' : '删除'}</button>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="items"><div class="empty-note">还没有代码片段。</div></div>
  {/if}
</div>

{#if editing}
  <Dialog title={editing.isNew ? '新建片段' : '编辑片段'} onclose={() => (editing = null)} width={520}>
    <form class="form" onsubmit={submit}>
      <label class="field"><span>名称</span><input class="input" bind:value={editing.s.name} required /></label>
      <label class="field">
        <span>命令（可以多行）</span>
        <textarea class="input" bind:value={editing.s.command} required></textarea>
      </label>
      <label class="check"><input type="checkbox" bind:checked={editing.s.run} />发送后自动回车执行</label>
      {#if error}<p class="error-box">{error}</p>{/if}
      <div class="dialog-actions">
        <button class="btn ghost" type="button" onclick={() => (editing = null)}>取消</button>
        <button class="btn" type="submit">保存</button>
      </div>
    </form>
  </Dialog>
{/if}

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
</style>
