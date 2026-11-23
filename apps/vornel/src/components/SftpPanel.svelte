<script lang="ts">
  import * as api from '../lib/api';
  import type { FileEntry } from '../lib/api';
  import Icon from './Icon.svelte';

  let { sessionId, onclose }: { sessionId: string; onclose: () => void } = $props();

  interface Side {
    path: string;
    entries: FileEntry[];
    selected: string | null;
    error: string;
    loading: boolean;
  }

  const blank = (): Side => ({ path: '', entries: [], selected: null, error: '', loading: false });
  let local = $state<Side>(blank());
  let remote = $state<Side>(blank());

  interface Transfer {
    id: number;
    name: string;
    dir: 'up' | 'down';
    done: number;
    total: number;
    error?: string;
    finished?: boolean;
  }
  let transfers = $state<Transfer[]>([]);
  let nextId = 0;

  const sep = (p: string) => (p.includes('\\') && !p.includes('/') ? '\\' : '/');
  const join = (dir: string, name: string) => (dir.endsWith('/') || dir.endsWith('\\') ? dir + name : dir + sep(dir) + name);
  function parent(p: string) {
    const s = sep(p);
    const trimmed = p.length > 1 ? p.replace(/[\\/]+$/, '') : p;
    const i = trimmed.lastIndexOf(s);
    if (i <= 0) return s === '/' ? '/' : trimmed.slice(0, 3);
    return trimmed.slice(0, i);
  }

  async function load(side: Side, isRemote: boolean, path: string) {
    side.loading = true;
    side.error = '';
    try {
      side.entries = isRemote ? await api.sftpList(sessionId, path) : await api.localList(path);
      side.path = path;
      side.selected = null;
    } catch (e) {
      side.error = api.errorText(e);
    } finally {
      side.loading = false;
    }
  }

  $effect(() => {
    void (async () => {
      try {
        const [lh, rh] = await Promise.all([api.localHome(), api.sftpHome(sessionId)]);
        await Promise.all([load(local, false, lh), load(remote, true, rh)]);
      } catch (e) {
        remote.error = api.errorText(e);
      }
    })();
  });

  function open(side: Side, isRemote: boolean, e: FileEntry) {
    if (e.isDir) void load(side, isRemote, join(side.path, e.name));
  }

  async function runTransfer(t: Transfer, job: (cb: (p: api.Progress) => void) => Promise<string>, after: () => Promise<void>) {
    transfers.unshift(t);
    const item = transfers[0];
    try {
      await job((p) => {
        item.done = p.done;
        item.total = p.total;
      });
      item.finished = true;
      await after();
    } catch (e) {
      item.error = api.errorText(e);
    }
  }

  function upload() {
    const f = local.entries.find((e) => e.name === local.selected);
    if (!f || f.isDir) return;
    void runTransfer(
      { id: nextId++, name: f.name, dir: 'up', done: 0, total: f.size },
      (cb) => api.sftpUpload(sessionId, join(local.path, f.name), remote.path, cb),
      () => load(remote, true, remote.path),
    );
  }

  function download() {
    const f = remote.entries.find((e) => e.name === remote.selected);
    if (!f || f.isDir) return;
    void runTransfer(
      { id: nextId++, name: f.name, dir: 'down', done: 0, total: f.size },
      (cb) => api.sftpDownload(sessionId, join(remote.path, f.name), local.path, cb),
      () => load(local, false, local.path),
    );
  }

  let editing = $state<{ mode: 'mkdir' | 'rename'; value: string } | null>(null);
  let confirmDelete = $state(false);

  async function applyEdit(e: SubmitEvent) {
    e.preventDefault();
    if (!editing || !editing.value.trim()) return;
    try {
      if (editing.mode === 'mkdir') {
        await api.sftpMkdir(sessionId, join(remote.path, editing.value.trim()));
      } else if (remote.selected) {
        await api.sftpRename(sessionId, join(remote.path, remote.selected), join(remote.path, editing.value.trim()));
      }
      editing = null;
      await load(remote, true, remote.path);
    } catch (err) {
      remote.error = api.errorText(err);
    }
  }

  async function del() {
    if (!remote.selected) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    confirmDelete = false;
    try {
      await api.sftpRemove(sessionId, join(remote.path, remote.selected));
      await load(remote, true, remote.path);
    } catch (err) {
      remote.error = api.errorText(err);
    }
  }

  const pct = (t: Transfer) => (t.total ? Math.round((t.done / t.total) * 100) : t.finished ? 100 : 0);
</script>

{#snippet column(side: Side, isRemote: boolean, title: string)}
  <section class="col" aria-label={title}>
    <div class="bar">
      <button class="icon-btn" type="button" aria-label="上一级" onclick={() => load(side, isRemote, parent(side.path))}>
        <Icon name="chevron" size={14} />
      </button>
      <input
        class="path mono"
        value={side.path}
        aria-label="{title}路径"
        onkeydown={(e) => e.key === 'Enter' && load(side, isRemote, (e.target as HTMLInputElement).value)}
      />
      <button class="icon-btn" type="button" aria-label="刷新" onclick={() => load(side, isRemote, side.path)}>
        <Icon name="refresh" size={14} />
      </button>
    </div>
    <div class="head"><span>{title}</span><span>大小</span><span>修改时间</span></div>
    <div class="list" role="listbox" aria-label="{title}文件">
      {#if side.error}<p class="err">{side.error}</p>{/if}
      {#each side.entries as e (e.name)}
        <button
          type="button"
          class="row"
          class:on={side.selected === e.name}
          role="option"
          aria-selected={side.selected === e.name}
          onclick={() => {
            side.selected = e.name;
            confirmDelete = false;
          }}
          ondblclick={() => open(side, isRemote, e)}
        >
          <span class="n">
            <span class:dir={e.isDir}><Icon name={e.isDir ? 'folder' : 'file'} size={15} /></span>
            <span class="mono nm">{e.name}</span>
          </span>
          <span class="s mono">{e.isDir ? '—' : api.formatSize(e.size)}</span>
          <span class="s mono">{api.formatTime(e.modified)}</span>
        </button>
      {/each}
    </div>
  </section>
{/snippet}

<div class="sftp">
  <div class="top">
    <b>SFTP</b>
    <span class="sp"></span>
    <button class="btn ghost" type="button" onclick={upload} disabled={!local.selected}>上传 →</button>
    <button class="btn ghost" type="button" onclick={download} disabled={!remote.selected}>← 下载</button>
    <button class="btn ghost" type="button" onclick={() => (editing = { mode: 'mkdir', value: '' })}>新建文件夹</button>
    <button
      class="btn ghost"
      type="button"
      disabled={!remote.selected}
      onclick={() => (editing = { mode: 'rename', value: remote.selected ?? '' })}>重命名</button
    >
    <button class="btn danger" type="button" disabled={!remote.selected} onclick={del}>{confirmDelete ? '确认删除？' : '删除'}</button>
    <button class="icon-btn" type="button" aria-label="关闭 SFTP" onclick={onclose}><Icon name="close" /></button>
  </div>

  {#if editing}
    <form class="edit" onsubmit={applyEdit}>
      <span>{editing.mode === 'mkdir' ? '新文件夹名称' : '新名称'}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input mono" bind:value={editing.value} autofocus />
      <button class="btn" type="submit">确定</button>
      <button class="btn ghost" type="button" onclick={() => (editing = null)}>取消</button>
    </form>
  {/if}

  <div class="cols">
    {@render column(local, false, '本机')}
    {@render column(remote, true, '远端')}
  </div>

  {#if transfers.length}
    <div class="xfers">
      {#each transfers.slice(0, 4) as t (t.id)}
        <div class="xfer">
          <div class="xt">
            <span>{t.dir === 'up' ? '上传' : '下载'} {t.name}</span>
            <span class="mono" class:errt={!!t.error}>{t.error ? t.error : t.finished ? '完成' : `${pct(t)}%`}</span>
          </div>
          <div class="bar2"><i style:width="{pct(t)}%"></i></div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .sftp {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--pane);
    min-width: 0;
  }
  .top {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .top .btn {
    height: 28px;
  }
  .sp {
    flex: 1;
  }
  .edit {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    font-size: 12.5px;
    color: var(--mut);
  }
  .edit .input {
    flex: 1;
  }
  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }
  .col + .col {
    border-left: 1px solid var(--line);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px;
  }
  .bar .icon-btn:first-child :global(svg) {
    transform: rotate(180deg);
  }
  .path {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--raise);
    font-size: 12px;
    outline: none;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 70px 70px;
    gap: 8px;
    align-items: center;
    padding: 0 10px;
  }
  .head {
    height: 28px;
    font-size: 11.5px;
    color: var(--mut);
    border-bottom: 1px solid var(--line);
  }
  .list {
    flex: 1;
    overflow: auto;
  }
  .row {
    width: 100%;
    height: 30px;
    border: 0;
    background: none;
    text-align: left;
    font-size: 12.5px;
  }
  .row:hover {
    background: var(--sel);
  }
  .row.on {
    background: var(--accbg);
  }
  .n {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    color: var(--mut);
  }
  .n .dir {
    color: var(--acc);
  }
  .nm {
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .s {
    font-size: 11.5px;
    color: var(--mut);
    text-align: right;
  }
  .err {
    margin: 10px;
    font-size: 12px;
    color: var(--err);
  }
  .xfers {
    border-top: 1px solid var(--line);
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .xt {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    margin-bottom: 4px;
  }
  .errt {
    color: var(--err);
  }
  .bar2 {
    height: 4px;
    border-radius: 2px;
    background: var(--sel);
  }
  .bar2 i {
    display: block;
    height: 4px;
    border-radius: 2px;
    background: var(--acc);
  }
</style>
