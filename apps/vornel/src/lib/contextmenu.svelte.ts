// 自定义右键菜单：任何地方调用 openMenu(e, items) 即可，全局只有一个菜单实例（components/ContextMenu.svelte）。
import type { IconName } from '../components/Icon.svelte';

export type MenuItem =
  | {
      label: string;
      icon?: IconName;
      /** 右侧显示的快捷键提示 */
      shortcut?: string;
      danger?: boolean;
      disabled?: boolean;
      /** 开关项：显示勾选状态 */
      checked?: boolean;
      action?: () => unknown;
      submenu?: MenuItem[];
    }
  | 'sep';

export const ctxMenu = $state<{ open: boolean; x: number; y: number; items: MenuItem[] }>({ open: false, x: 0, y: 0, items: [] });

export function openMenu(e: MouseEvent, items: MenuItem[]) {
  e.preventDefault();
  e.stopPropagation();
  if (!items.length) return;
  ctxMenu.items = items;
  ctxMenu.x = e.clientX;
  ctxMenu.y = e.clientY;
  ctxMenu.open = true;
}

export function closeMenu() {
  ctxMenu.open = false;
}

export const isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);
/** 快捷键提示：macOS 用 ⌘，其他平台用 Ctrl+ */
export const mod = (k: string) => (isMac ? `⌘${k}` : `Ctrl+${k}`);
