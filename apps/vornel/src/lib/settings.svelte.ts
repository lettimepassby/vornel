import { DEFAULT_PACK } from './fonts';
import type { ThemeId } from './themes';

export interface Settings {
  theme: ThemeId;
  fontPack: string;
  terminalFontSize: number;
}

const KEY = 'vornel.settings';
const DEFAULTS: Settings = { theme: 'ink', fontPack: DEFAULT_PACK, terminalFontSize: 13 };

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...DEFAULTS, ...JSON.parse(raw) };
  } catch {
    // 读取失败就用默认值
  }
  return { ...DEFAULTS };
}

/** 全局设置（响应式）。云同步在 M3 接入，目前保存在本机。 */
export const settings = $state<Settings>(load());

export function saveSettings() {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {
    // 存储不可用时忽略
  }
}
