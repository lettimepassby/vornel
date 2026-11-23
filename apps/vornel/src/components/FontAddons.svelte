<script lang="ts">
  import { errorText, formatSize } from '../lib/api';
  import { installAddon, listAddons, removeAddon, type Addon } from '../lib/addons';
  import { prefs } from '../lib/store.svelte';

  let addons = $state<Addon[]>([]);
  let progress = $state<Record<string, number>>({});
  let error = $state('');
  // iOS 上下载前先确认（ARCHITECTURE.md 8.3：可能使用蜂窝流量）
  const isIOS = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  let confirmId = $state<string | null>(null);

  async function refresh() {
    addons = await listAddons();
  }

  $effect(() => {
    void refresh().catch((e) => (error = errorText(e)));
  });

  async function install(a: Addon) {
    if (isIOS && confirmId !== a.id) {
      confirmId = a.id;
      return;
    }
    confirmId = null;
    error = '';
    progress[a.id] = 0;
    try {
      await installAddon(a.id, (p) => (progress[a.id] = p.total ? Math.round((p.done / p.total) * 100) : 0));
      await refresh();
    } catch (e) {
      error = errorText(e);
    } finally {
      delete progress[a.id];
    }
  }

  async function uninstall(a: Addon) {
    await removeAddon(a.id);
    // 正在使用的字体被删除时退回字体包默认值
    const next = { ...prefs.fontOverrides };
    for (const role of ['terminal', 'cjk']) if (next[role] === a.family) delete next[role];
    prefs.fontOverrides = next;
    await refresh();
  }

  function use(a: Addon) {
    prefs.fontOverrides = { ...prefs.fontOverrides, [a.role]: a.family };
  }

  const inUse = (a: Addon) => prefs.fontOverrides[a.role] === a.family;
</script>

<div class="block">
  <b>中文增强包</b>
  <span class="sub">按需下载，原版字体转换为 woff2、不做子集化，下载后校验 sha256。使用蜂窝网络时请注意流量。</span>
  <div class="items">
    {#each addons as a (a.id)}
      <div class="item">
        <div class="main">
          <b>{a.family} <span class="tag">{a.role === 'terminal' ? '终端' : '界面中文'}</span></b>
          <small>{a.size ? formatSize(a.size) : '大小未知'} · OFL-1.1</small>
        </div>
        <div class="acts">
          {#if a.id in progress}
            <span class="mono pct">{progress[a.id]}%</span>
          {:else if a.installed}
            {#if inUse(a)}
              <span class="tag ok">使用中</span>
            {:else}
              <button class="btn ghost" type="button" onclick={() => use(a)}>使用</button>
            {/if}
            <button class="btn danger" type="button" onclick={() => uninstall(a)}>删除</button>
          {:else}
            <button class="btn" type="button" onclick={() => install(a)}>
              {confirmId === a.id ? `确认下载 ${formatSize(a.size)}（可能使用蜂窝流量）` : '下载'}
            </button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
  {#if error}<p class="error-box">{error}</p>{/if}
</div>

<style>
  .block {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  b {
    font-weight: 600;
    font-size: 13.5px;
  }
  .sub {
    font-size: 12px;
    color: var(--mut);
  }
  .pct {
    font-size: 12px;
    color: var(--acc);
    min-width: 40px;
    text-align: right;
  }
</style>
