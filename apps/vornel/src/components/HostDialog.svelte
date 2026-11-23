<script lang="ts">
  import { untrack } from 'svelte';
  import * as api from '../lib/api';
  import { PROTOCOL_LABELS, PROTOCOL_PORTS, type Host, type Protocol } from '../lib/api';
  import { data } from '../lib/store.svelte';
  import Dialog from './Dialog.svelte';

  let {
    host,
    isNew,
    onsave,
    ondelete,
    onclose,
  }: {
    host: Host;
    isNew: boolean;
    onsave: (h: Host) => Promise<void>;
    ondelete: (h: Host) => Promise<void>;
    onclose: () => void;
  } = $props();

  // 编辑副本：只在打开对话框时取一次初始值
  let draft = $state<Host>(untrack(() => $state.snapshot(host)) as Host);
  let authType = $state(untrack(() => host.auth.type));
  let keyId = $state(untrack(() => (host.auth.type === 'key' ? host.auth.keyId : '')));
  let keyPath = $state(untrack(() => (host.auth.type === 'keyFile' ? host.auth.path : '~/.ssh/id_ed25519')));
  let error = $state('');
  let busy = $state(false);
  let confirmDelete = $state(false);

  const groups = $derived([...new Set(data.hosts.map((h) => h.group).filter((g): g is string => !!g))].sort());
  const jumpCandidates = $derived(data.hosts.filter((h) => h.id !== draft.id && h.protocol === 'ssh'));
  const isSshLike = $derived(draft.protocol === 'ssh' || draft.protocol === 'mosh');
  const PROTOCOLS = Object.keys(PROTOCOL_LABELS) as Protocol[];

  function setProtocol(p: Protocol) {
    const wasDefault = draft.port === PROTOCOL_PORTS[draft.protocol];
    draft.protocol = p;
    if (wasDefault || !draft.port) draft.port = PROTOCOL_PORTS[p];
    if (p === 'serial' && !draft.serial) {
      draft.serial = { baudRate: 115200, dataBits: 8, parity: 'none', stopBits: 1, flowControl: 'none' };
    }
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      const auth: Host['auth'] =
        authType === 'key'
          ? { type: 'key', keyId }
          : authType === 'keyFile'
            ? { type: 'keyFile', path: keyPath.trim() }
            : authType === 'agent'
              ? { type: 'agent' }
              : { type: 'password' };
      if (authType === 'key' && !keyId) throw '请选择一把密钥';
      const h: Host = {
        ...draft,
        name: draft.name.trim() || draft.address.trim(),
        address: draft.address.trim(),
        username: draft.username.trim(),
        group: draft.group?.trim() || null,
        port: Number(draft.port) || 0,
        auth,
        jumpHost: isSshLike ? draft.jumpHost || null : null,
        savePassword: authType === 'password' ? draft.savePassword : false,
        serial: draft.protocol === 'serial' ? draft.serial : null,
        domain: draft.protocol === 'rdp' ? draft.domain?.trim() || null : null,
      };
      await onsave(h);
    } catch (err) {
      error = api.errorText(err);
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
      error = api.errorText(err);
      busy = false;
    }
  }
</script>

<Dialog title={isNew ? '新建主机' : `编辑 ${host.name}`} {onclose} width={540}>
  <form onsubmit={submit}>
    <div class="field">
      <span>协议</span>
      <div class="seg wrap" role="group" aria-label="协议">
        {#each PROTOCOLS as p (p)}
          <button type="button" aria-pressed={draft.protocol === p} onclick={() => setProtocol(p)}>{PROTOCOL_LABELS[p]}</button>
        {/each}
      </div>
    </div>

    <div class="grid">
      <label class="field span2">
        <span>{draft.protocol === 'serial' ? '串口设备' : '地址'}</span>
        <input
          class="input mono"
          bind:value={draft.address}
          placeholder={draft.protocol === 'serial' ? '/dev/tty.usbserial-0001 或 COM3' : '10.0.1.21 或 example.com'}
          required
        />
      </label>
      {#if draft.protocol !== 'serial'}
        <label class="field"><span>端口</span><input class="input mono" type="number" min="1" max="65535" bind:value={draft.port} /></label>
      {:else if draft.serial}
        <label class="field">
          <span>波特率</span>
          <select class="input mono" bind:value={draft.serial.baudRate}>
            {#each [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600] as b (b)}<option value={b}>{b}</option>{/each}
          </select>
        </label>
      {/if}
      <label class="field span2"><span>名称</span><input class="input mono" bind:value={draft.name} placeholder="留空则使用地址" /></label>
      {#if draft.protocol !== 'serial'}
        <label class="field">
          <span>用户名{draft.protocol === 'telnet' || draft.protocol === 'vnc' ? '（可选）' : ''}</span>
          <input class="input mono" bind:value={draft.username} />
        </label>
      {/if}
      {#if draft.protocol === 'serial' && draft.serial}
        <label class="field">
          <span>数据位 / 校验 / 停止位</span>
          <div class="trio">
            <select class="input mono" bind:value={draft.serial.dataBits}>{#each [5, 6, 7, 8] as b (b)}<option value={b}>{b}</option>{/each}</select>
            <select class="input" bind:value={draft.serial.parity}>
              <option value="none">无</option><option value="odd">奇</option><option value="even">偶</option>
            </select>
            <select class="input mono" bind:value={draft.serial.stopBits}><option value={1}>1</option><option value={2}>2</option></select>
          </div>
        </label>
        <label class="field">
          <span>流控</span>
          <select class="input" bind:value={draft.serial.flowControl}>
            <option value="none">无</option><option value="software">软件（XON/XOFF）</option><option value="hardware">硬件（RTS/CTS）</option>
          </select>
        </label>
      {/if}
      {#if draft.protocol === 'rdp'}
        <label class="field span3"><span>域（可选）</span><input class="input mono" bind:value={draft.domain} /></label>
      {/if}
      <label class="field span3">
        <span>分组</span>
        <input class="input" list="vornel-groups" bind:value={draft.group} placeholder="例如：生产环境" />
        <datalist id="vornel-groups">{#each groups as g (g)}<option value={g}></option>{/each}</datalist>
      </label>
    </div>

    {#if isSshLike}
      <div class="field">
        <span>认证方式</span>
        <div class="seg" role="group" aria-label="认证方式">
          <button type="button" aria-pressed={authType === 'password'} onclick={() => (authType = 'password')}>密码</button>
          <button type="button" aria-pressed={authType === 'key'} onclick={() => (authType = 'key')}>密钥串</button>
          <button type="button" aria-pressed={authType === 'keyFile'} onclick={() => (authType = 'keyFile')}>私钥文件</button>
          <button type="button" aria-pressed={authType === 'agent'} onclick={() => (authType = 'agent')}>SSH Agent</button>
        </div>
      </div>
      {#if authType === 'password'}
        <label class="check"><input type="checkbox" bind:checked={draft.savePassword} />在 Vault 中保存密码（首次连接时输入）</label>
      {:else if authType === 'key'}
        <label class="field">
          <span>密钥</span>
          <select class="input" bind:value={keyId}>
            <option value="" disabled>{data.keys.length ? '选择一把密钥' : '密钥串为空，请先生成或导入'}</option>
            {#each data.keys as k (k.id)}<option value={k.id}>{k.name} · {k.algorithm}</option>{/each}
          </select>
        </label>
      {:else if authType === 'keyFile'}
        <label class="field"><span>私钥文件</span><input class="input mono" bind:value={keyPath} placeholder="~/.ssh/id_ed25519" required /></label>
      {:else}
        <p class="note">使用系统 SSH Agent 中的密钥（ssh-agent、1Password、Windows OpenSSH Agent 等）。</p>
      {/if}

      <div class="grid2">
        <label class="field">
          <span>跳板机</span>
          <select class="input" bind:value={draft.jumpHost}>
            <option value={null}>不使用</option>
            {#each jumpCandidates as h (h.id)}<option value={h.id}>{h.name}</option>{/each}
          </select>
        </label>
        <label class="check agent"><input type="checkbox" bind:checked={draft.agentForward} />转发本机 SSH Agent</label>
      </div>
    {:else if draft.protocol === 'rdp' || draft.protocol === 'vnc'}
      <label class="check"><input type="checkbox" bind:checked={draft.savePassword} />在 Vault 中保存密码</label>
    {/if}

    {#if error}<p class="error-box" role="alert">{error}</p>{/if}

    <div class="actions">
      {#if !isNew}
        <button class="btn danger" type="button" onclick={remove} disabled={busy}>{confirmDelete ? '确认删除？' : '删除'}</button>
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
  .grid2 {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    align-items: end;
  }
  .span2 {
    grid-column: span 2;
  }
  .span3 {
    grid-column: span 3;
  }
  .seg.wrap {
    flex-wrap: wrap;
  }
  .trio {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }
  .trio .input {
    padding: 0 6px;
  }
  .agent {
    height: 32px;
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--mut);
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
