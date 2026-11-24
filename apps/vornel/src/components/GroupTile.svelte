<script lang="ts">
  // 分组卡片的图标：自定义图片 > 系统 logo > 通用图标 > 默认分组图标
  import type { GroupStyle } from '../lib/api';
  import { osIcon } from '../lib/os-icons';
  import Icon, { type IconName } from './Icon.svelte';

  let { style, size = 38 }: { style: GroupStyle | undefined; size?: number } = $props();

  const os = $derived(style?.icon?.startsWith('os:') ? osIcon(style.icon.slice(3)) : null);
  const icon = $derived((style?.icon?.startsWith('icon:') ? style.icon.slice(5) : 'group') as IconName);
  const bg = $derived(style?.image ? 'transparent' : (style?.color ?? os?.bg ?? 'color-mix(in srgb, var(--acc) 85%, #000)'));
  const fg = $derived(os && !style?.color ? os.fg : '#fff');
</script>

<span class="tile" style:width="{size}px" style:height="{size}px" style:background={bg} style:color={fg}>
  {#if style?.image}
    <img src={style.image} alt="" />
  {:else if os}
    <svg viewBox="0 0 24 24" aria-hidden="true" width={size * 0.55} height={size * 0.55}><path d={os.path} fill="currentColor" /></svg>
  {:else}
    <Icon name={icon} size={Math.round(size * 0.52)} />
  {/if}
</span>

<style>
  .tile {
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 9px;
    overflow: hidden;
    transition: transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
