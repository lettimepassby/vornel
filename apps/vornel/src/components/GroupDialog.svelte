<script lang="ts">
  // 分组外观：改名、选图标（系统 logo / 通用图标）、底色、上传自定义图片
  import { untrack } from 'svelte';
  import * as api from '../lib/api';
  import type { GroupStyle } from '../lib/api';
  import { OS_ICON_IDS, osIcon } from '../lib/os-icons';
  import { data, loadAll, prefs } from '../lib/store.svelte';
  import Dialog from './Dialog.svelte';
  import GroupTile from './GroupTile.svelte';
  import Icon, { type IconName } from './Icon.svelte';

  let { group, onclose, onrenamed }: { group: string; onclose: () => void; onrenamed: (name: string) => void } = $props();

  const GENERIC: IconName[] = ['group', 'host', 'cloud', 'home', 'building', 'database', 'globe', 'flask', 'box', 'terminal', 'monitor', 'shield', 'star', 'code'];
  const COLORS = ['#4353D1', '#2563EB', '#0891B2', '#059669', '#65A30D', '#CA8A04', '#EA580C', '#DC2626', '#DB2777', '#9333EA', '#57534E', '#1F2937'];

  let name = $state(untrack(() => group));
  let style = $state<GroupStyle>(untrack(() => ({ ...(prefs.groupStyles?.[group] ?? {}) })));
  let error = $state('');
  let busy = $state(false);
  let fileInput = $state<HTMLInputElement>();

  function pick(icon: string) {
    style = { ...style, icon, image: null };
  }

  /** 把图片缩成 96×96 的 PNG，保存在偏好里随同步走 */
  async function upload(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    error = '';
    try {
      const url = URL.createObjectURL(file);
      const img = new Image();
      await new Promise((ok, fail) => {
        img.onload = ok;
        img.onerror = () => fail(new Error('无法读取这张图片'));
        img.src = url;
      });
      const c = document.createElement('canvas');
      c.width = c.height = 96;
      const s = Math.min(img.width, img.height);
      c.getContext('2d')!.drawImage(img, (img.width - s) / 2, (img.height - s) / 2, s, s, 0, 0, 96, 96);
      URL.revokeObjectURL(url);
      const png = c.toDataURL('image/png');
      if (png.length > 64 * 1024) throw new Error('图片太复杂，请换一张简单些的 logo');
      style = { ...style, image: png };
    } catch (err) {
      error = api.errorText(err);
    }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    const next = name.trim();
    if (!next) {
      error = '分组名称不能为空';
      return;
    }
    busy = true;
    try {
      if (next !== group) {
        if (data.hosts.some((h) => h.group?.trim() === next)) throw new Error(`已经有名为“${next}”的分组`);
        for (const h of data.hosts.filter((h) => h.group?.trim() === group)) await api.save('host', { ...$state.snapshot(h), group: next });
      }
      const styles = { ...prefs.groupStyles };
      delete styles[group];
      if (style.icon || style.color || style.image) styles[next] = $state.snapshot(style);
      prefs.groupStyles = styles;
      // 先直接写入 Vault，避免随后的 loadAll 读到旧的偏好把改动覆盖掉
      await api.save('prefs', $state.snapshot(prefs));
      await loadAll();
      if (next !== group) onrenamed(next);
      onclose();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title="分组外观" {onclose} width={520}>
  <form onsubmit={save}>
    <div class="head">
      <GroupTile {style} size={54} />
      <label class="field grow"><span>分组名称</span><input class="input" bind:value={name} /></label>
    </div>

    <div class="field">
      <span>系统 logo</span>
      <div class="icons" role="group" aria-label="系统 logo">
        {#each OS_ICON_IDS as id (id)}
          {@const os = osIcon(id)!}
          <button type="button" class="ic" aria-pressed={style.icon === `os:${id}` && !style.image} aria-label={os.name} title={os.name} onclick={() => pick(`os:${id}`)}>
            <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d={os.path} fill={os.bg === '#2C2C30' ? 'currentColor' : os.bg} /></svg>
          </button>
        {/each}
      </div>
    </div>

    <div class="field">
      <span>通用图标</span>
      <div class="icons" role="group" aria-label="通用图标">
        {#each GENERIC as g (g)}
          <button type="button" class="ic" aria-pressed={(style.icon ?? 'icon:group') === `icon:${g}` && !style.image} aria-label={g} onclick={() => pick(`icon:${g}`)}>
            <Icon name={g} size={17} />
          </button>
        {/each}
      </div>
    </div>

    <div class="field">
      <span>底色</span>
      <div class="colors" role="group" aria-label="底色">
        <button type="button" class="sw auto" aria-pressed={!style.color} aria-label="默认底色" title="默认" onclick={() => (style = { ...style, color: null })}></button>
        {#each COLORS as c (c)}
          <button type="button" class="sw" style:background={c} aria-pressed={style.color === c} aria-label="底色 {c}" onclick={() => (style = { ...style, color: c })}></button>
        {/each}
      </div>
    </div>

    <div class="field">
      <span>自定义图片</span>
      <div class="row">
        <button class="btn ghost" type="button" onclick={() => fileInput?.click()}><Icon name="upload" size={14} />上传图片…</button>
        {#if style.image}<button class="link" type="button" onclick={() => (style = { ...style, image: null })}>移除图片</button>{/if}
        <span class="sub">比如云厂商或公司的 logo，会裁成正方形并缩小保存。</span>
      </div>
      <input bind:this={fileInput} type="file" accept="image/*" hidden onchange={upload} />
    </div>

    {#if error}<p class="error-box" role="alert">{error}</p>{/if}

    <div class="dialog-actions">
      <span class="grow"></span>
      <button class="btn ghost" type="button" onclick={onclose}>取消</button>
      <button class="btn" type="submit" disabled={busy}>保存</button>
    </div>
  </form>
</Dialog>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .head {
    display: flex;
    align-items: flex-end;
    gap: 14px;
  }
  .grow {
    flex: 1;
  }
  .icons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .ic {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border: 1px solid var(--line2);
    border-radius: 8px;
    background: var(--raise);
    color: var(--fg);
    cursor: pointer;
    transition:
      transform 120ms ease-out,
      border-color 120ms ease-out;
  }
  .ic:hover {
    transform: translateY(-1px);
  }
  .ic[aria-pressed='true'] {
    border-color: var(--acc);
    box-shadow: 0 0 0 2px var(--accbg);
  }
  .colors {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .sw {
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 50%;
    cursor: pointer;
    transition: transform 120ms ease-out;
  }
  .sw:hover {
    transform: scale(1.12);
  }
  .sw[aria-pressed='true'] {
    box-shadow:
      0 0 0 2px var(--raise),
      0 0 0 4px var(--acc);
  }
  .sw.auto {
    background: conic-gradient(var(--acc), #059669, #ea580c, var(--acc));
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .sub {
    font-size: 12px;
    color: var(--mut);
  }
</style>
