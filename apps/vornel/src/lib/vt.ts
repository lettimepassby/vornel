// 视图切换过渡：支持 View Transitions 的 WebView（Windows WebView2、较新的 macOS）里，
// 主机页 ↔ 终端、标签切换时新旧画面交叉淡入并轻微缩放；不支持时直接切换（CSS 里另有淡入兜底）。
import { flushSync } from 'svelte';

type VTDocument = Document & { startViewTransition?: (cb: () => void) => unknown };

// 自动化测试（WebDriver）驱动时不使用过渡：部分 WebView 在过渡期间会把新界面判为不可交互
export const supportsViewTransition =
  typeof document !== 'undefined' && 'startViewTransition' in document && !navigator.webdriver;

export function vt(fn: () => void) {
  const d = document as VTDocument;
  if (!supportsViewTransition || !d.startViewTransition || matchMedia('(prefers-reduced-motion: reduce)').matches) {
    fn();
    return;
  }
  const t = d.startViewTransition(() => {
    fn();
    flushSync();
  }) as { skipTransition?: () => void; finished?: Promise<void> } | undefined;
  // 保险：过渡超过 600ms 还没结束（个别环境下渲染被节流）就直接跳过，界面不能一直处于过渡中
  const guard = setTimeout(() => t?.skipTransition?.(), 600);
  void t?.finished?.finally(() => clearTimeout(guard));
}
