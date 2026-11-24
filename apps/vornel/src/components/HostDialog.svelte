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
    onsave: (h: Host, password?: string) => Promise<void>;
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
  /** 新填写的密码（留空 = 不修改） */
  let password = $state('');
  /** 已保存过密码、用户点了“清除” */
  let clearPassword = $state(false);
  const hadPassword = untrack(() => !isNew && host.savePassword);
  /** 分组：下拉选择已有分组，或“新建分组”后输入名称 */
  const NEW_GROUP = '\u0000new';
  let groupChoice = $state(untrack(() => host.group ?? ''));
  let newGroup = $state('');
  let newGroupInput = $state<HTMLInputElement>();
  $effect(() => {
    if (groupChoice === NEW_GROUP) newGroupInput?.focus();
  });

  const groups = $derived([...new Set(data.hosts.map((h) => h.group).filter((g): g is string => !!g))].sort());
  const jumpCandidates = $derived(data.hosts.filter((h) => h.id !== draft.id && h.protocol === 'ssh'));
  const isSshLike = $derived(draft.protocol === 'ssh' || draft.protocol === 'mosh');
  const PROTOCOLS = Object.keys(PROTOCOL_LABELS) as Protocol[];
  let ports = $state<string[]>([]);
  $effect(() => {
    if (draft.protocol === 'serial') void api.serialPorts().then((p) => (ports = p)).catch(() => {});
  });

  function setProtocol(p: Protocol) {
    const wasDefault = draft.port === PROTOCOL_PORTS[draft.protocol];
    draft.protocol = p;
    if (wasDefault || !draft.port) draft.port = PROTOCOL_PORTS[p];
    // VNC / 串口不使用用户名，别把 SSH 的默认用户名 root 带过去
    if ((p === 'vnc' || p === 'serial') && draft.username === 'root') draft.username = '';
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
      if (groupChoice === NEW_GROUP && !newGroup.trim()) throw '请填写新分组的名称';
      const usesPassword = (isSshLike && authType === 'password') || draft.protocol === 'rdp' || draft.protocol === 'vnc';
      const savePassword = usesPassword && !!(password || (hadPassword && !clearPassword));
      const h: Host = {
        ...draft,
        name: draft.name.trim() || draft.address.trim(),
        address: draft.address.trim(),
        username: draft.username.trim(),
        group: (groupChoice === NEW_GROUP ? newGroup : groupChoice).trim() || null,
        port: Number(draft.port) || 0,
        auth,
        jumpHost: isSshLike ? draft.jumpHost || null : null,
        savePassword,
        serial: draft.protocol === 'serial' ? draft.serial : null,
        domain: draft.protocol === 'rdp' ? draft.domain?.trim() || null : null,
      };
      await onsave(h, savePassword && password ? password : undefined);
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
          list={draft.protocol === 'serial' ? 'vornel-ports' : undefined}
          required
        />
        <datalist id="vornel-ports">{#each ports as p (p)}<option value={p}></option>{/each}</datalist>
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
      <label class="field" class:span3={groupChoice !== NEW_GROUP}>
        <span>分组</span>
        <select class="input" bind:value={groupChoice} aria-label="分组">
          <option value="">未分组</option>
          {#each groups as g (g)}<option value={g}>{g}</option>{/each}
          <option value={NEW_GROUP}>＋ 新建分组…</option>
        </select>
      </label>
      {#if groupChoice === NEW_GROUP}
        <label class="field span2">
          <span>新分组名称</span>
          <input class="input" bind:this={newGroupInput} bind:value={newGroup} placeholder="例如：生产环境" />
        </label>
      {/if}
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
<label class="field">
          <span>密码</span>
          <input
            class="input mono"
            type="password"
            bind:value={password}
            autocomplete="new-password"
            placeholder={hadPassword && !clearPassword ? '已保存（留空则不修改）' : '留空则连接时再输入'}
          />
        </label>
        {#if hadPassword && !clearPassword}
          <button class="link clear" type="button" onclick={() => ((clearPassword = true), (password = ''))}>清除已保存的密码</button>
        {/if}
        <p class="note">密码用本机数据密钥加密后保存在 Vault 里。</p>
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
<label class="field">
        <span>密码</span>
          <input
            class="input mono"
            type="password"
            bind:value={password}
            autocomplete="new-password"
            placeholder={hadPassword && !clearPassword ? '已保存（留空则不修改）' : '留空则连接时再输入'}
          />
        </label>
        {#if hadPassword && !clearPassword}
          <button class="link clear" type="button" onclick={() => ((clearPassword = true), (password = ''))}>清除已保存的密码</button>
        {/if}
        <p class="note">密码用本机数据密钥加密后保存在 Vault 里。</p>
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
  .clear {
    align-self: flex-start;
    margin-top: -8px;
    font-size: 12px;
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
