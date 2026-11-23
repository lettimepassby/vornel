<script lang="ts">
  import * as api from '../lib/api';
  import type { Key } from '../lib/api';
  import { data, loadAll } from '../lib/store.svelte';
  import Dialog from '../components/Dialog.svelte';
  import Icon from '../components/Icon.svelte';

  let mode = $state<'generate' | 'import' | null>(null);
  let name = $state('');
  let pem = $state('');
  let passphrase = $state('');
  let error = $state('');
  let busy = $state(false);
  let copied = $state<string | null>(null);
  let confirmId = $state<string | null>(null);

  function openDialog(m: 'generate' | 'import') {
    mode = m;
    name = '';
    pem = '';
    passphrase = '';
    error = '';
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      if (mode === 'generate') await api.keyGenerate(name);
      else await api.keyImport(name, pem, passphrase);
      mode = null;
      await loadAll();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }

  async function copy(k: Key) {
    await navigator.clipboard.writeText(k.publicKey);
    copied = k.id;
    setTimeout(() => (copied = null), 1500);
  }

  async function toggleSync(k: Key) {
    await api.save('key', { ...k, sync: !k.sync });
    await loadAll();
  }

  async function del(k: Key) {
    if (confirmId !== k.id) {
      confirmId = k.id;
      return;
    }
    await api.remove('key', k.id);
    confirmId = null;
    await loadAll();
  }

  const usedBy = (k: Key) => data.hosts.filter((h) => h.auth.type === 'key' && h.auth.keyId === k.id).length;
</script>

<div class="page">
  <div class="page-head">
    <h2>密钥串</h2>
    <span class="n">{data.keys.length}</span>
    <span class="sp"></span>
    <button class="btn ghost" type="button" onclick={() => openDialog('import')}>导入私钥</button>
    <button class="btn" type="button" onclick={() => openDialog('generate')}><Icon name="plus" size={14} stroke={2} />生成密钥</button>
  </div>
  <p class="page-sub">
    私钥加密保存在本机 Vault 中，永远不会显示在界面上。默认<strong>只保存在本机</strong>；开启“同步”后才会加密上传到云端。
  </p>

  {#if data.keys.length}
    <div class="items">
      {#each data.keys as k (k.id)}
        <div class="item">
          <div class="main">
            <b>{k.name} <span class="tag">{k.algorithm}</span> {#if k.encrypted}<span class="tag">有口令</span>{/if}</b>
            <small>{k.fingerprint} · 被 {usedBy(k)} 台主机使用</small>
          </div>
          <div class="acts">
            <label class="check" title="同步到云端"><input type="checkbox" checked={k.sync} onchange={() => toggleSync(k)} />同步</label>
            <button class="btn ghost" type="button" onclick={() => copy(k)}>
              <Icon name="copy" size={14} />{copied === k.id ? '已复制' : '复制公钥'}
            </button>
            <button class="btn danger" type="button" onclick={() => del(k)}>{confirmId === k.id ? '确认删除？' : '删除'}</button>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="items"><div class="empty-note">还没有密钥。生成一把 ed25519 密钥，或导入已有的私钥。</div></div>
  {/if}
</div>

{#if mode}
  <Dialog title={mode === 'generate' ? '生成 ed25519 密钥' : '导入私钥'} onclose={() => (mode = null)} width={520}>
    <form class="form" onsubmit={submit}>
      <label class="field">
        <span>名称</span>
        <input class="input" bind:value={name} placeholder={mode === 'generate' ? '例如：macbook-2026' : '留空则使用算法名'} />
      </label>
      {#if mode === 'import'}
        <label class="field">
          <span>私钥（OpenSSH / PEM 格式）</span>
          <textarea class="input" bind:value={pem} placeholder="-----BEGIN OPENSSH PRIVATE KEY-----" required></textarea>
        </label>
        <label class="field">
          <span>口令（私钥有口令时填写）</span>
          <input class="input" type="password" bind:value={passphrase} />
        </label>
      {:else}
        <p class="page-sub" style="margin:0">生成后可以“复制公钥”，粘贴到服务器的 <code>~/.ssh/authorized_keys</code>。</p>
      {/if}
      {#if error}<p class="error-box">{error}</p>{/if}
      <div class="dialog-actions">
        <button class="btn ghost" type="button" onclick={() => (mode = null)}>取消</button>
        <button class="btn" type="submit" disabled={busy}>{mode === 'generate' ? '生成' : '导入'}</button>
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
