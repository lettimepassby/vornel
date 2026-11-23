<script lang="ts" module>
  // 24 网格、线宽 1.7、圆角端点，见 DESIGN.md 第 6 节
  const PATHS = {
    host: 'M5 4h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM5 13h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2zM7 7.5h.01M7 16.5h.01',
    key: 'M8 11a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM10.8 12.2 20 3M16 7l3 3',
    code: 'M8 7l-5 5 5 5M16 7l5 5-5 5',
    forward: 'M4 8h14l-3-3M20 16H6l3 3',
    shield: 'M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6zM9 12l2 2 4-4',
    search: 'M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-3.5-3.5',
    cloud: 'M7 18a5 5 0 1 1 .9-9.9A6 6 0 0 1 19 10a4 4 0 0 1-1 8z',
    settings: 'M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM12 2v3M12 19v3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M2 12h3M19 12h3M4.9 19.1 7 17M17 7l2.1-2.1',
    plus: 'M12 5v14M5 12h14',
    close: 'M7 7l10 10M17 7 7 17',
    edit: 'M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4',
    trash: 'M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3',
    terminal: 'M5 8l4 4-4 4M12 17h7',
    split: 'M5.5 4h13A2.5 2.5 0 0 1 21 6.5v11a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5v-11A2.5 2.5 0 0 1 5.5 4zM12 4v16',
    folder: 'M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2h7.5A2.5 2.5 0 0 1 21 9.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5z',
    refresh: 'M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7',
    chevron: 'M9 6l6 6-6 6',
    file: 'M6 3h8l4 4v14H6zM14 3v4h4',
    play: 'M7 5l12 7-12 7z',
    stop: 'M6 6h12v12H6z',
    copy: 'M9 9h11v11H9zM5 15H4V4h11v1',
    upload: 'M12 16V4M6 10l6-6 6 6M4 20h16',
    monitor: 'M3 5h18v12H3zM8 21h8M12 17v4',
    send: 'M4 12l16-8-6 16-3-7z',
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 16, stroke = 1.7 }: { name: IconName; size?: number; stroke?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={PATHS[name]} />
</svg>
