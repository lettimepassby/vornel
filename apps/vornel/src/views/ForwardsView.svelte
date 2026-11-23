<script lang="ts">
  import * as api from '../lib/api';
  import type { Forward, ForwardKind } from '../lib/api';
  import { data, hostById, loadAll } from '../lib/store.svelte';
  import Dialog from '../components/Dialog.svelte';
  import Icon from '../components/Icon.svelte';

  const KIND_LABEL: Record<ForwardKind, string> = {
    local: '本地 -L',
    remote: '远程 -R',
    dynamic: 'SOCKS5 -D',
  };

  let editing = $state<{ f: Forward; isNew: boolean } | null>(null);
  let error = $state('');
  let rowError = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);
  let askPassword = $state<{ f: Forward; value: string } | null>(null);
  let confirmId = $state<string | null>(null);

  const sshHosts = $derived(data.hosts.filter((h) => h.protocol === 'ssh'));

  function edit(f?: Forward) {
    error = '';
    editing = f
      ? { f: { ...f }, isNew: false }
      : {
          f: {
            id: crypto.randomUUID(),
            name: '',
            hostId: sshHosts[0]?.id ?? '',
            kind: 'local',
            bindAddress: '127.0.0.1',
            bindPort: 8080,
            targetHost: '127.0.0.1',
            targetPort: 80,
          },
          isNew: true,
        };
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!editing) return;
    try {
      const f = editing.f;
      await api.save('forward', {
        ...f,
        name: f.name.trim() || `${KIND_LABEL[f.kind]} ${f.bindPort}`,
        bindPort: Number(f.bindPort),
        targetPort: Number(f.targetPort) || 0,
      });
      editing = null;
      await loadAll();
    } catch (err) {
      error = api.errorText(err);
    }
  }

  async function start(f: Forward, password?: string) {
    busy = f.id;
    rowError[f.id] = '';
    try {
      await api.forwardStart(f.id, password);
      askPassword = null;
      await loadAll();
    } catch (err) {
      const m = api.errorText(err);
      if (m.startsWith('NEED_PASSWORD')) askPassword = { f, value: '' };
      else rowError[f.id] = m;
    } finally {
      busy = null;
    }
  }

  async function stop(f: Forward) {
    await api.forwardStop(f.id);
    await loadAll();
  }

  async function del(f: Forward) {
    if (confirmId !== f.id) {
      confirmId = f.id;
      return;
    }
    await api.forwardStop(f.id);
    await api.remove('forward', f.id);
    confirmId = null;
    await loadAll();
  }

  function describe(f: Forward) {
    const bind = `${f.bindAddress}:${f.bindPort}`;
    if (f.kind === 'dynamic') return `SOCKS5 代理 ${bind}`;
    const target = `${f.targetHost}:${f.targetPort}`;
    return f.kind === 'local' ? `本机 ${bind} → 远端 ${target}` : `远端 ${bind} → 本机 ${target}`;
  }
</script>

<div class="page">
  <div class="page-head">
    <h2>端口转发</h2>
    <span class="n">{data.forwards.length}</span>
    <span class="sp"></span>
    <button class="btn" type="button" onclick={() => edit()} disabled={!sshHosts.length}>
      <Icon name="plus" size={14} stroke={2} />新建规则
    </button>
  </div>
  <p class="page-sub">
    本地转发把本机端口映射到远端可达的地址；远程转发反过来；SOCKS5 动态转发让浏览器等程序经由远端上网。每条规则使用独立的 SSH 连接。
  </p>

  {#if data.forwards.length}
    <div class="items">
      {#each data.forwards as f (f.id)}
        {@const running = data.runningForwards.includes(f.id)}
        <div class="item">
          <span class="dot" class:ok={running}></span>
          <div class="main">
            <b>{f.name} <span class="tag">{KIND_LABEL[f.kind]}</span> {#if running}<span class="tag ok">运行中</span>{/if}</b>
            <small>{describe(f)} · 经由 {hostById(f.hostId)?.name ?? '（主机已删除）'}</small>
            {#if rowError[f.id]}<small class="e">{rowError[f.id]}</small>{/if}
          </div>
          <div class="acts">
            {#if running}
              <button class="btn ghost" type="button" onclick={() => stop(f)}><Icon name="stop" size={13} />停止</button>
            {:else}
              <button class="btn" type="button" onclick={() => start(f)} disabled={busy === f.id}>
                <Icon name="play" size={13} />{busy === f.id ? '连接中…' : '启动'}
              </button>
            {/if}
            <button class="icon-btn" type="button" aria-label="编辑 {f.name}" onclick={() => edit(f)}><Icon name="edit" size={15} /></button>
            <button class="btn danger" type="button" onclick={() => del(f)}>{confirmId === f.id ? '确认删除？' : '删除'}</button>
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="items">
      <div class="empty-note">{sshHosts.length ? '还没有端口转发规则。' : '先添加一台 SSH 主机，才能创建端口转发。'}</div>
    </div>
  {/if}
</div>

{#if editing}
  <Dialog title={editing.isNew ? '新建端口转发' : '编辑端口转发'} onclose={() => (editing = null)} width={520}>
    <form class="form" onsubmit={submit}>
      <div class="form-grid">
        <label class="field full"><span>名称</span><input class="input" bind:value={editing.f.name} placeholder="例如：数据库" /></label>
        <label class="field">
          <span>经由主机</span>
          <select class="input" bind:value={editing.f.hostId}>
            {#each sshHosts as h (h.id)}<option value={h.id}>{h.name}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span>类型</span>
          <select class="input" bind:value={editing.f.kind}>
            <option value="local">本地转发（-L）</option>
            <option value="remote">远程转发（-R）</option>
            <option value="dynamic">SOCKS5 动态转发（-D）</option>
          </select>
        </label>
        <label class="field">
          <span>{editing.f.kind === 'remote' ? '远端监听地址' : '本机监听地址'}</span>
          <input class="input mono" bind:value={editing.f.bindAddress} />
        </label>
        <label class="field"><span>监听端口</span><input class="input mono" type="number" min="1" max="65535" bind:value={editing.f.bindPort} /></label>
        {#if editing.f.kind !== 'dynamic'}
          <label class="field">
            <span>{editing.f.kind === 'local' ? '目标地址（从远端看）' : '目标地址（从本机看）'}</span>
            <input class="input mono" bind:value={editing.f.targetHost} />
          </label>
          <label class="field"><span>目标端口</span><input class="input mono" type="number" min="1" max="65535" bind:value={editing.f.targetPort} /></label>
        {/if}
      </div>
      {#if error}<p class="error-box">{error}</p>{/if}
      <div class="dialog-actions">
        <button class="btn ghost" type="button" onclick={() => (editing = null)}>取消</button>
        <button class="btn" type="submit">保存</button>
      </div>
    </form>
  </Dialog>
{/if}

{#if askPassword}
  <Dialog title="输入 {hostById(askPassword.f.hostId)?.name} 的密码" onclose={() => (askPassword = null)} width={380}>
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        if (askPassword) void start(askPassword.f, askPassword.value);
      }}
    >
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" type="password" bind:value={askPassword.value} autofocus />
      <div class="dialog-actions"><button class="btn" type="submit">启动</button></div>
    </form>
  </Dialog>
{/if}

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .e {
    color: var(--err) !important;
  }
</style>
