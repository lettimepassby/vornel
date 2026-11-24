<script lang="ts">
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { untrack } from 'svelte';
  import * as api from '../lib/api';
  import type { BackendConfig } from '../lib/api';
  import { ensureFamily, familyStack, FONT_PACKS } from '../lib/fonts';
  import { FONT_LICENSES, LIBRARIES } from '../lib/licenses';
  import { loadAll, prefs } from '../lib/store.svelte';
  import { checkForUpdate, installUpdate, loadCurrentVersion, updater, updaterSupported } from '../lib/updater.svelte';
  import Dialog from './Dialog.svelte';
  import FontAddons from './FontAddons.svelte';
  import Icon from './Icon.svelte';
  import { copyText } from '../lib/clipboard';

  let { onclose, initialTab = 'appearance' }: { onclose: () => void; initialTab?: Tab } = $props();

  type Tab = 'appearance' | 'sync' | 'security' | 'about';
  let tab = $state<Tab>(untrack(() => initialTab));

  // 打开设置时预加载所有字体包，让预览卡片显示真实字体
  $effect(() => {
    for (const p of FONT_PACKS) {
      void ensureFamily(p.ui);
      void ensureFamily(p.terminal);
    }
  });

  $effect(() => {
    void loadCurrentVersion();
  });

  function setSize(delta: number) {
    prefs.terminalFontSize = Math.min(24, Math.max(10, prefs.terminalFontSize + delta));
  }

  const ALL_UI = [...new Set(FONT_PACKS.map((p) => p.ui))];
  let systemFonts = $state<{ name: string; monospace: boolean }[]>([]);
  $effect(() => {
    void api.systemFonts().then((f) => (systemFonts = f)).catch(() => {});
  });
  const systemMono = $derived(systemFonts.filter((f) => f.monospace));
  const ALL_MONO = [...new Set(FONT_PACKS.flatMap((p) => [p.mono, p.terminal]))];

  function setOverride(role: 'ui' | 'terminal', value: string) {
    const next = { ...prefs.fontOverrides };
    if (value) next[role] = value;
    else delete next[role];
    prefs.fontOverrides = next;
  }

  // ---------------------------------------------------------------- 同步
  type BackendType = BackendConfig['type'];
  const BACKENDS: { type: BackendType; label: string }[] = [
    { type: 'folder', label: '本地文件夹' },
    { type: 's3', label: 'S3 兼容' },
    { type: 'webdav', label: 'WebDAV' },
    { type: 'gdrive', label: 'Google Drive' },
    { type: 'oneDrive', label: 'OneDrive' },
    { type: 'dropbox', label: 'Dropbox' },
  ];

  let cfg = $state<BackendConfig | null>(null);
  let backendType = $state<BackendType>('folder');
  let form = $state({
    path: '',
    endpoint: '',
    bucket: '',
    region: '',
    accessKeyId: '',
    secretAccessKey: '',
    username: '',
    password: '',
    root: '',
    clientId: '',
    clientSecret: '',
  });
  let syncMsg = $state('');
  let syncErr = $state('');
  let syncBusy = $state(false);
  let masterPw = $state('');
  let masterPw2 = $state('');
  let needPw = $state<'new' | 'existing' | null>(null);
  /** 本机已加入云端同步库（之后才能修改主密码） */
  let joined = $state(false);
  let copied = $state(false);

  /** 查询云端状态：需要设置 / 输入主密码时直接显示密码框 */
  async function refreshCheck() {
    try {
      const c = await api.syncCheck();
      needPw = c.needPassword === null ? null : c.needPassword ? 'new' : 'existing';
      joined = c.configured && c.needPassword === null;
    } catch {
      joined = false;
    }
  }

  async function copyError() {
    await copyText(syncErr);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  $effect(() => {
    void api.syncGetConfig().then((c) => {
      cfg = c;
      if (c) {
        backendType = c.type;
        Object.assign(form, c);
        void refreshCheck();
      }
    });
  });

  const isOAuth = $derived(backendType === 'gdrive' || backendType === 'oneDrive' || backendType === 'dropbox');

  function buildConfig(): BackendConfig {
    switch (backendType) {
      case 'folder':
        return { type: 'folder', path: form.path };
      case 's3':
        return {
          type: 's3',
          endpoint: form.endpoint,
          bucket: form.bucket,
          region: form.region,
          accessKeyId: form.accessKeyId,
          secretAccessKey: form.secretAccessKey,
          root: form.root,
        };
      case 'webdav':
        return { type: 'webdav', endpoint: form.endpoint, username: form.username, password: form.password, root: form.root };
      default:
        throw new Error('请点击“授权”完成登录');
    }
  }

  async function pickFolder() {
    const dir = await openDialog({ directory: true, title: '选择同步文件夹' });
    if (typeof dir === 'string') form.path = dir;
  }

  async function saveBackend() {
    syncErr = '';
    syncMsg = '';
    syncBusy = true;
    try {
      const c = buildConfig();
      await api.syncSetConfig(c);
      cfg = c;
      await refreshCheck();
      syncMsg = needPw === 'new'
        ? '连接测试通过。请设置主密码，然后点击“立即同步”。'
        : needPw === 'existing'
          ? '连接测试通过，云端已有同步库。请输入主密码后点击“立即同步”。'
          : '连接测试通过，已保存。';
    } catch (e) {
      syncErr = api.errorText(e);
    } finally {
      syncBusy = false;
    }
  }

  async function authorize() {
    syncErr = '';
    syncMsg = '已在浏览器中打开授权页面，请完成登录…';
    syncBusy = true;
    try {
      const provider = backendType === 'gdrive' ? 'google' : backendType === 'oneDrive' ? 'microsoft' : 'dropbox';
      cfg = await api.syncOAuth(provider, form.clientId, form.clientSecret);
      await refreshCheck();
      syncMsg = needPw === 'new' ? '授权成功。请设置主密码，然后点击“立即同步”。' : '授权成功。点击“立即同步”开始。';
    } catch (e) {
      syncMsg = '';
      syncErr = api.errorText(e);
    } finally {
      syncBusy = false;
    }
  }

  async function disconnect() {
    await api.syncSetConfig(null);
    cfg = null;
    joined = false;
    needPw = null;
    syncMsg = '已断开同步，本机数据保留。';
  }

  async function syncNow() {
    syncErr = '';
    syncMsg = '';
    if (needPw === 'new' && masterPw !== masterPw2) {
      syncErr = '两次输入的主密码不一致';
      return;
    }
    if (needPw === 'new' && masterPw.length < 8) {
      syncErr = '主密码至少 8 个字符';
      return;
    }
    syncBusy = true;
    try {
      const r = await api.syncNow(needPw ? masterPw : undefined);
      needPw = null;
      joined = true;
      masterPw = masterPw2 = '';
      syncMsg = `${r.created ? '已创建云端同步库。' : ''}${r.joined ? '已加入已有的同步库。' : ''}拉取 ${r.pulled} 条，推送 ${r.pushed} 条。`;
      await loadAll();
    } catch (e) {
      const m = api.errorText(e);
      if (m.startsWith('NEED_PASSWORD:')) {
        needPw = m.endsWith('new') ? 'new' : 'existing';
      } else {
        syncErr = m;
      }
    } finally {
      syncBusy = false;
    }
  }

  let newPw = $state('');
  let newPw2 = $state('');
  let pwMsg = $state('');
  async function changePassword() {
    pwMsg = '';
    if (newPw !== newPw2) {
      pwMsg = '两次输入的主密码不一致';
      return;
    }
    try {
      await api.syncChangePassword(newPw);
      newPw = newPw2 = '';
      pwMsg = '已修改。已加入的设备不受影响，之后新加入的设备请使用新密码。';
    } catch (e) {
      pwMsg = api.errorText(e);
    }
  }

  // ---------------------------------------------------------------- 安全
  let confirmReset = $state(false);
  async function resetVault() {
    if (!confirmReset) {
      confirmReset = true;
      return;
    }
    await api.vaultReset();
    confirmReset = false;
    await loadAll();
  }

  let openLicense = $state<string | null>(null);
</script>

<Dialog title="设置" width={700} {onclose}>
  <div class="tabs seg" role="tablist" aria-label="设置分类">
    <button type="button" role="tab" aria-selected={tab === 'appearance'} onclick={() => (tab = 'appearance')}>外观</button>
    <button type="button" role="tab" aria-selected={tab === 'sync'} onclick={() => (tab = 'sync')}>同步</button>
    <button type="button" role="tab" aria-selected={tab === 'security'} onclick={() => (tab = 'security')}>安全</button>
    <button type="button" role="tab" aria-selected={tab === 'about'} onclick={() => (tab = 'about')}>关于与开源许可</button>
  </div>

  {#if tab === 'appearance'}
    <section>
      <div class="row">
        <b>主题</b>
        <div class="seg" role="group" aria-label="主题">
          <button type="button" aria-pressed={prefs.theme === 'ink'} onclick={() => (prefs.theme = 'ink')}>深色 Ink</button>
          <button type="button" aria-pressed={prefs.theme === 'paper'} onclick={() => (prefs.theme = 'paper')}>浅色 Paper</button>
        </div>
      </div>

      <div class="block">
        <b>字体包</b>
        <span class="sub">英文字体已内置、离线可用；界面中文默认使用系统字体（苹方 / 微软雅黑）。</span>
        <div class="packs">
          {#each FONT_PACKS as p (p.id)}
            <button type="button" class="pk" class:on={prefs.fontPack === p.id} aria-pressed={prefs.fontPack === p.id} onclick={() => (prefs.fontPack = p.id)}>
              <span class="pv">
                <span class="a" style:font-family={familyStack(p.ui, 'sans-serif')}>Aa 主机</span>
                <span class="t" style:font-family={familyStack(p.terminal, 'monospace')}>❯ ssh web-01</span>
              </span>
              <span class="nm">{p.name}{#if p.addon}<span class="tag acc">+ {p.addon}</span>{/if}</span>
              <span class="fs">{p.ui} · {p.terminal}</span>
            </button>
          {/each}
        </div>
      </div>

      <div class="block">
        <b>单独调整</b>
        <div class="row">
          <span>界面字体</span>
          <select class="input sel" value={prefs.fontOverrides.ui ?? ''} onchange={(e) => setOverride('ui', (e.target as HTMLSelectElement).value)}>
            <option value="">跟随字体包</option>
            <optgroup label="内置">{#each ALL_UI as f (f)}<option value={f}>{f}</option>{/each}</optgroup>
            {#if systemFonts.length}
              <optgroup label="本机已安装">{#each systemFonts as f (f.name)}<option value={f.name}>{f.name}</option>{/each}</optgroup>
            {/if}
          </select>
        </div>
        <div class="row">
          <span>终端字体</span>
          <select class="input sel" value={prefs.fontOverrides.terminal ?? ''} onchange={(e) => setOverride('terminal', (e.target as HTMLSelectElement).value)}>
            <option value="">跟随字体包</option>
            <optgroup label="内置">
              {#each ALL_MONO as f (f)}<option value={f}>{f}</option>{/each}
              <option value="Maple Mono CN">Maple Mono CN（需下载增强包）</option>
            </optgroup>
            {#if systemMono.length}
              <optgroup label="本机已安装（等宽）">{#each systemMono as f (f.name)}<option value={f.name}>{f.name}</option>{/each}</optgroup>
            {/if}
          </select>
        </div>
        <div class="row">
          <span>终端字号</span>
          <div class="stepper">
            <button class="icon-btn" type="button" aria-label="减小字号" onclick={() => setSize(-1)}>−</button>
            <span class="mono">{prefs.terminalFontSize}</span>
            <button class="icon-btn" type="button" aria-label="增大字号" onclick={() => setSize(1)}>+</button>
          </div>
        </div>
      </div>

      <label class="check big">
        <input type="checkbox" bind:checked={prefs.copyOnSelect} />
        <span><b>选中即复制</b><br /><span class="sub">在终端里用鼠标选中文字后自动复制到剪贴板。关闭后可用 ⌘C（Windows 为 Ctrl+Shift+C）复制。</span></span>
      </label>

      <label class="check big">
        <input type="checkbox" bind:checked={prefs.screenReader} />
        <span><b>屏幕阅读器模式</b><br /><span class="sub">让 VoiceOver / 讲述人可以朗读终端内容。终端字号也可以用 ⌘+ / ⌘− / ⌘0 调整。</span></span>
      </label>

      <FontAddons />
    </section>
  {:else if tab === 'sync'}
    <section>
      <p class="sub">
        端到端加密：数据在本机用你的主密码派生的密钥加密后才上传，云端只能看到密文。主机、代码片段、端口转发、已知主机、偏好设置会同步；私钥和密码默认只保存在本机。
      </p>
      <div class="field">
        <span>同步位置</span>
        <div class="seg wrap" role="group" aria-label="同步后端">
          {#each BACKENDS as b (b.type)}
            <button type="button" aria-pressed={backendType === b.type} onclick={() => (backendType = b.type)}>{b.label}</button>
          {/each}
        </div>
      </div>

      {#if backendType === 'folder'}
        <div class="row gap">
          <input class="input mono grow" bind:value={form.path} placeholder="选择 iCloud Drive、Syncthing 或网盘同步目录" />
          <button class="btn ghost" type="button" onclick={pickFolder}>选择…</button>
        </div>
      {:else if backendType === 's3'}
        <div class="form-grid">
          <label class="field full"><span>Endpoint（AWS 可留空；R2 / MinIO 必填）</span><input class="input mono" bind:value={form.endpoint} placeholder="https://<account>.r2.cloudflarestorage.com" /></label>
          <label class="field"><span>Bucket</span><input class="input mono" bind:value={form.bucket} /></label>
          <label class="field"><span>Region</span><input class="input mono" bind:value={form.region} placeholder="auto" /></label>
          <label class="field"><span>Access Key ID</span><input class="input mono" bind:value={form.accessKeyId} /></label>
          <label class="field"><span>Secret Access Key</span><input class="input mono" type="password" bind:value={form.secretAccessKey} /></label>
          <label class="field full"><span>目录前缀（可选）</span><input class="input mono" bind:value={form.root} /></label>
        </div>
      {:else if backendType === 'webdav'}
        <div class="form-grid">
          <label class="field full"><span>WebDAV 地址</span><input class="input mono" bind:value={form.endpoint} placeholder="https://dav.example.com/remote.php/dav/files/me" /></label>
          <label class="field"><span>用户名</span><input class="input mono" bind:value={form.username} /></label>
          <label class="field"><span>密码 / 应用密码</span><input class="input mono" type="password" bind:value={form.password} /></label>
          <label class="field full"><span>目录（可选）</span><input class="input mono" bind:value={form.root} /></label>
        </div>
      {:else}
        <p class="sub">
          需要你在对应平台的开发者控制台注册一个“桌面应用”类型的 OAuth 客户端，回调地址允许 <code>http://127.0.0.1</code>。Vornel 只申请访问自己创建的文件的权限。
        </p>
        <div class="form-grid">
          <label class="field"><span>Client ID</span><input class="input mono" bind:value={form.clientId} /></label>
          <label class="field"><span>Client Secret（如平台提供）</span><input class="input mono" type="password" bind:value={form.clientSecret} /></label>
        </div>
      {/if}

      {#if needPw}
        <div class="pw">
          <b>{needPw === 'new' ? '设置主密码' : '输入主密码'}</b>
          <span class="sub">
            {needPw === 'new'
              ? '云端还没有同步库。主密码用来加密同步数据，其他设备加入时需要输入。请牢记，忘记后无法找回。'
              : '云端已有同步库，输入创建它时设置的主密码以加入。'}
          </span>
          <input class="input" type="password" bind:value={masterPw} placeholder="主密码" />
          {#if needPw === 'new'}<input class="input" type="password" bind:value={masterPw2} placeholder="再输入一次" />{/if}
        </div>
      {/if}

      {#if syncErr}
        <div class="error-box selectable">
          <p>{syncErr}</p>
          <button class="btn ghost copy" type="button" onclick={copyError}>{copied ? '已复制' : '复制错误信息'}</button>
        </div>
      {/if}
      {#if syncMsg}<p class="ok">{syncMsg}</p>{/if}

      <div class="dialog-actions">
        {#if cfg}<button class="btn danger" type="button" onclick={disconnect}>断开同步</button>{/if}
        <span class="grow"></span>
        {#if isOAuth}
          <button class="btn ghost" type="button" onclick={authorize} disabled={syncBusy}>授权</button>
        {:else}
          <button class="btn ghost" type="button" onclick={saveBackend} disabled={syncBusy}>{syncBusy ? '正在测试连接…' : '保存并测试'}</button>
        {/if}
        <button class="btn" type="button" onclick={syncNow} disabled={!cfg || syncBusy}>
          <Icon name="refresh" size={14} />{syncBusy ? '同步中…' : '立即同步'}
        </button>
      </div>

      {#if cfg && joined && !needPw}
        <div class="block">
          <b>修改主密码</b>
          <span class="sub">只会用新密码重新加密数据密钥，数据本身不变；已加入的设备不需要重新输入。</span>
          <div class="row gap">
            <input class="input grow" type="password" bind:value={newPw} placeholder="新主密码（至少 8 个字符）" />
            <input class="input grow" type="password" bind:value={newPw2} placeholder="再输入一次" />
            <button class="btn ghost" type="button" onclick={changePassword} disabled={!newPw}>修改</button>
          </div>
          {#if pwMsg}<p class="sub selectable">{pwMsg}</p>{/if}
        </div>
      {/if}
    </section>
  {:else if tab === 'security'}
    <section>
      <label class="check big">
        <input type="checkbox" bind:checked={prefs.syncSecrets} />
        <span><b>同步保存的密码</b><br /><span class="sub">开启后，保存在 Vault 中的主机密码也会加密上传到云端。私钥是否同步在“密钥串”中单独设置。</span></span>
      </label>
      <div class="block">
        <b>本机 Vault</b>
        <span class="sub">
          所有数据用随机生成的数据密钥加密，数据密钥保存在系统钥匙串（macOS Keychain / Windows 凭据管理器）。重置会清空本机所有数据；开启了同步的话可以重新同步回来。
        </span>
        <div><button class="btn danger" type="button" onclick={resetVault}>{confirmReset ? '确认清空本机数据？' : '重置本机 Vault'}</button></div>
      </div>
    </section>
  {:else}
    <section>
      <div class="block">
        <div class="row">
          <span><b>Vornel {updater.current}</b></span>
          {#if updaterSupported}
            <button class="btn ghost" type="button" onclick={() => checkForUpdate()} disabled={updater.checking || updater.downloading}>
              {updater.checking ? '正在检查…' : '检查更新'}
            </button>
          {/if}
        </div>
        {#if updaterSupported}
          {#if updater.update}
            <div class="pw">
              <div class="row">
                <span><b>发现新版本 {updater.update.version}</b></span>
                <button class="btn" type="button" onclick={installUpdate} disabled={updater.downloading}>
                  {updater.downloading ? `正在下载${updater.progress === null ? '…' : `（${updater.progress}%）`}` : '更新并重启'}
                </button>
              </div>
              {#if updater.update.body}<pre class="notes">{updater.update.body}</pre>{/if}
            </div>
          {:else if updater.checked && !updater.checking && !updater.error}
            <p class="ok">已是最新版本。</p>
          {/if}
          {#if updater.error}<p class="sub err">{updater.error}</p>{/if}
        {/if}
      </div>
      <p class="sub">
        源码以 PolyForm Noncommercial 1.0.0 许可公开，禁止商业使用。<br />
        Copyright © 2026 lettimepassby · github.com/lettimepassby/vornel
      </p>
      <div class="block">
        <b>内置字体（SIL Open Font License 1.1）</b>
        <div class="items">
          {#each FONT_LICENSES as f (f.name)}
            <div class="item col">
              <button class="lic-head" type="button" aria-expanded={openLicense === f.name} onclick={() => (openLicense = openLicense === f.name ? null : f.name)}>
                <b>{f.name}</b><span class="tag">OFL-1.1</span><span class="grow"></span><span class="sub">{openLicense === f.name ? '收起' : '查看全文'}</span>
              </button>
              {#if openLicense === f.name}<pre class="lic">{f.text}</pre>{/if}
            </div>
          {/each}
        </div>
      </div>
      <div class="block">
        <b>主要依赖</b>
        <div class="items">
          {#each LIBRARIES as l (l.name)}
            <div class="item"><div class="main"><b>{l.name}</b><small>{l.url}</small></div><span class="tag">{l.license}</span></div>
          {/each}
        </div>
      </div>
    </section>
  {/if}
</Dialog>

<style>
  .tabs {
    margin-bottom: 18px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .row.gap {
    justify-content: flex-start;
  }
  .grow {
    flex: 1;
  }
  b {
    font-weight: 600;
    font-size: 13.5px;
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sub {
    margin: 0;
    font-size: 12px;
    color: var(--mut);
    line-height: 1.6;
  }
  .sel {
    width: 240px;
  }
  .seg.wrap {
    flex-wrap: wrap;
    align-self: flex-start;
  }
  .packs {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
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
    gap: 6px;
    font-weight: 600;
    padding: 0 2px;
  }
  .nm .tag {
    font-size: 10px;
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
  .pw {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    border-radius: 10px;
    background: var(--accbg);
  }
  .ok {
    margin: 0;
    font-size: 12.5px;
    color: var(--ok);
  }
  .check.big {
    align-items: flex-start;
    color: var(--fg);
  }
  .check.big input {
    margin-top: 3px;
  }
  .item.col {
    flex-direction: column;
    align-items: stretch;
    padding: 0;
  }
  .lic-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border: 0;
    background: none;
    text-align: left;
  }
  .notes {
    margin: 0;
    max-height: 180px;
    overflow: auto;
    font: 12px/1.6 var(--f-ui);
    white-space: pre-wrap;
    color: var(--mut);
    user-select: text;
  }
  .sub.err {
    color: var(--err);
  }
  .lic {
    margin: 0;
    padding: 12px 14px;
    max-height: 260px;
    overflow: auto;
    border-top: 1px solid var(--line);
    font: 11px/1.6 var(--f-mono);
    white-space: pre-wrap;
    color: var(--mut);
    user-select: text;
  }
</style>
