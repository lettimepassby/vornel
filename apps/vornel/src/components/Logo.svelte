<script lang="ts">
  // v▌ 标志，几何见 DESIGN.md 2.1
  let { size = 20, wordmark = false }: { size?: number; wordmark?: boolean } = $props();
</script>

<span class="logo" style:gap="{Math.round(size * 0.26)}px">
  <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
    <path
      d="M12 22 L24 45 L36 22"
      fill="none"
      stroke="currentColor"
      stroke-width="7"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <rect x="42" y="17" width="10" height="30" rx="2.5" fill="var(--acc)" />
  </svg>
  {#if wordmark}<span class="word" style:font-size="{Math.round(size * 0.8)}px">vornel</span>{/if}
</span>

<style>
  .logo {
    display: inline-flex;
    align-items: center;
    color: var(--fg);
  }
  .word {
    font-weight: 600;
    letter-spacing: -0.04em;
  }
</style>
