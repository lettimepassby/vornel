<script lang="ts">
  import { errorText, type Host } from '../lib/api';
  import Dialog from './Dialog.svelte';

  let {
    host,
    isNew,
    groups,
    onsave,
    ondelete,
    onclose,
  }: {
    host: Host;
    isNew: boolean;
    groups: string[];
    onsave: (h: Host) => Promise<void>;
    ondelete: (h: Host) => Promise<void>;
    onclose: () => void;
  } = $props();

  // 编辑副本，保存前不影响列表
  let draft = $state<Host>(structuredClone($state.snapshot(host)) as Host);
  let keyPath = $state(host.auth.type === 'keyFile' ? host.auth.path : '~/.ssh/id_ed25519');
  let error = $state('');
  let busy = $state(false);
  let confirmDelete = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      const h: Host = {
        ...draft,
        name: draft.name.trim() || draft.address.trim(),
        address: draft.address.trim(),
        username: draft.username.trim(),
        group: draft.group?.trim() || null,
        port: Number(draft.port) || 22,
        auth: draft.auth.type === 'keyFile' ? { type: 'keyFile', path: keyPath.trim() } : { type: 'password' },
      };
      await onsave(h);
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    busy = true;
    try {
      await ondelete(host);
    } catch (err) {
      error = errorText(err);
      busy = false;
    }
  }
</script>

<Dialog title={isNew ? '新建主机' : `编辑 ${host.name}`} {onclose}>
  <form onsubmit={submit}>
    <div class="grid">
      <label class="field span2">
        <span>地址</span>
        <input class="input mono" bind:value={draft.address} placeholder="10.0.1.21 或 example.com" required />
      </label>
      <label class="field">
        <span>端口</span>
        <input class="input mono" type="number" min="1" max="65535" bind:value={draft.port} />
      </label>
      <label class="field span2">
        <span>名称</span>
        <input class="input mono" bind:value={draft.name} placeholder="留空则使用地址" />
      </label>
      <label class="field">
        <span>用户名</span>
        <input class="input mono" bind:value={draft.username} required />
      </label>
      <label class="field span3">
        <span>分组</span>
        <input class="input" list="vornel-groups" bind:value={draft.group} placeholder="例如：生产环境" />
        <datalist id="vornel-groups">
          {#each groups as g (g)}<option value={g}></option>{/each}
        </datalist>
      </label>
    </div>

    <div class="field auth">
      <span>认证方式</span>
      <div class="seg" role="group" aria-label="认证方式">
        <button type="button" aria-pressed={draft.auth.type === 'password'} onclick={() => (draft.auth = { type: 'password' })}>密码</button>
        <button type="button" aria-pressed={draft.auth.type === 'keyFile'} onclick={() => (draft.auth = { type: 'keyFile', path: keyPath })}>私钥</button>
      </div>
    </div>
    {#if draft.auth.type === 'keyFile'}
      <label class="field">
        <span>私钥文件</span>
        <input class="input mono" bind:value={keyPath} placeholder="~/.ssh/id_ed25519" required />
      </label>
    {:else}
      <p class="note">密码不会保存，每次连接时输入。加密保存与云同步将在 M3 加入。</p>
    {/if}

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="actions">
      {#if !isNew}
        <button class="btn danger" type="button" onclick={remove} disabled={busy}>
          {confirmDelete ? '确认删除？' : '删除'}
        </button>
      {/if}
      <span class="sp"></span>
      <button class="btn ghost" type="button" onclick={onclose}>取消</button>
      <button class="btn" type="submit" disabled={busy}>{isNew ? '添加' : '保存'}</button>
    </div>
  </form>
</Dialog>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }
  .span2 {
    grid-column: span 2;
  }
  .span3 {
    grid-column: span 3;
  }
  .auth {
    align-items: flex-start;
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--mut);
  }
  .error {
    margin: 0;
    padding: 8px 10px;
    border-radius: 7px;
    background: color-mix(in srgb, var(--err) 12%, transparent);
    color: var(--err);
    font-size: 12.5px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  .actions .btn {
    height: 32px;
  }
  .sp {
    flex: 1;
  }
</style>
