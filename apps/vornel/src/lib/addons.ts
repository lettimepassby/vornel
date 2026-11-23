// 中文字体增强包：列出、下载、删除，并把已安装的字体注册到字体系统。
import { Channel, invoke } from '@tauri-apps/api/core';
import type { Progress } from './api';
import { registerAddonFamily } from './fonts';

export interface AddonFile {
  name: string;
  weight: string;
}

export interface Addon {
  id: string;
  family: string;
  role: 'terminal' | 'cjk';
  size: number;
  installed: boolean;
  files: AddonFile[];
}

/** Windows 上自定义协议走 http://<scheme>.localhost，其他平台是 <scheme>://localhost */
function fontUrl(id: string, file: string) {
  const path = `${id}/${encodeURIComponent(file)}`;
  return /Windows/.test(navigator.userAgent) ? `http://vornel-font.localhost/${path}` : `vornel-font://localhost/${path}`;
}

export async function listAddons(): Promise<Addon[]> {
  const addons = await invoke<Addon[]>('font_addons');
  for (const a of addons) {
    if (a.installed) {
      registerAddonFamily(
        a.family,
        a.files.map((f) => ({ url: fontUrl(a.id, f.name), weight: f.weight })),
      );
    }
  }
  return addons;
}

export function installAddon(id: string, onProgress: (p: Progress) => void) {
  const ch = new Channel<Progress>();
  ch.onmessage = onProgress;
  return invoke<void>('font_addon_install', { id, onProgress: ch });
}

export const removeAddon = (id: string) => invoke<void>('font_addon_remove', { id });
