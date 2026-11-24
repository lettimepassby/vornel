// 剪贴板：桌面端走原生剪贴板（读剪贴板不会弹系统确认），其他情况退回 WebView 接口。
import { invoke } from '@tauri-apps/api/core';

export async function copyText(text: string): Promise<void> {
  if (!text) return;
  try {
    await invoke('clipboard_write', { text });
    return;
  } catch {
    // iOS 等：用 WebView 接口
  }
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // 再退回 execCommand
  }
  const ta = document.createElement('textarea');
  ta.value = text;
  ta.style.cssText = 'position:fixed;left:-9999px;top:0;opacity:0';
  const prev = document.activeElement as HTMLElement | null;
  document.body.appendChild(ta);
  ta.select();
  try {
    document.execCommand('copy');
  } finally {
    ta.remove();
    prev?.focus();
  }
}

export async function readText(): Promise<string> {
  try {
    return await invoke<string>('clipboard_read');
  } catch {
    return navigator.clipboard.readText().catch(() => '');
  }
}
