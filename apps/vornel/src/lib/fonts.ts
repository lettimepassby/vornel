// 字体包：内置英文字体（只含拉丁字符子集的 woff2），选中哪个字体包才加载哪个。
// 清单与说明见 design/font-packs.json 与 DESIGN.md 第 4 节。

import geist from '@fontsource-variable/geist/files/geist-latin-wght-normal.woff2?url';
import geistMono from '@fontsource-variable/geist-mono/files/geist-mono-latin-wght-normal.woff2?url';
import inter from '@fontsource-variable/inter/files/inter-latin-wght-normal.woff2?url';
import jetbrainsMono from '@fontsource-variable/jetbrains-mono/files/jetbrains-mono-latin-wght-normal.woff2?url';
import manrope from '@fontsource-variable/manrope/files/manrope-latin-wght-normal.woff2?url';
import instrumentSans from '@fontsource-variable/instrument-sans/files/instrument-sans-latin-wght-normal.woff2?url';
import spaceGrotesk from '@fontsource-variable/space-grotesk/files/space-grotesk-latin-wght-normal.woff2?url';
import martianMono from '@fontsource-variable/martian-mono/files/martian-mono-latin-wght-normal.woff2?url';
import cascadiaCode from '@fontsource-variable/cascadia-code/files/cascadia-code-latin-wght-normal.woff2?url';
import plexSans400 from '@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-400-normal.woff2?url';
import plexSans500 from '@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-500-normal.woff2?url';
import plexSans600 from '@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-600-normal.woff2?url';
import plexMono400 from '@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-400-normal.woff2?url';
import plexMono500 from '@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-500-normal.woff2?url';
import plexMono600 from '@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-600-normal.woff2?url';
import commitMono400 from '@fontsource/commit-mono/files/commit-mono-latin-400-normal.woff2?url';
import commitMono700 from '@fontsource/commit-mono/files/commit-mono-latin-700-normal.woff2?url';
import maple400 from '@fontsource/maple-mono/files/maple-mono-latin-400-normal.woff2?url';
import maple700 from '@fontsource/maple-mono/files/maple-mono-latin-700-normal.woff2?url';
import monaspace400 from '@fontsource/monaspace-neon/files/monaspace-neon-latin-400-normal.woff2?url';
import monaspace500 from '@fontsource/monaspace-neon/files/monaspace-neon-latin-500-normal.woff2?url';
import monaspace600 from '@fontsource/monaspace-neon/files/monaspace-neon-latin-600-normal.woff2?url';

interface Face {
  url: string;
  /** 可变字体写范围，例如 '100 900'。 */
  weight: string;
}

const VARIABLE = '100 900';

const FACES: Record<string, Face[]> = {
  Geist: [{ url: geist, weight: VARIABLE }],
  'Geist Mono': [{ url: geistMono, weight: VARIABLE }],
  Inter: [{ url: inter, weight: VARIABLE }],
  'JetBrains Mono': [{ url: jetbrainsMono, weight: VARIABLE }],
  Manrope: [{ url: manrope, weight: VARIABLE }],
  'Instrument Sans': [{ url: instrumentSans, weight: VARIABLE }],
  'Space Grotesk': [{ url: spaceGrotesk, weight: VARIABLE }],
  'Martian Mono': [{ url: martianMono, weight: VARIABLE }],
  'Cascadia Code': [{ url: cascadiaCode, weight: VARIABLE }],
  'IBM Plex Sans': [
    { url: plexSans400, weight: '400' },
    { url: plexSans500, weight: '500' },
    { url: plexSans600, weight: '600' },
  ],
  'IBM Plex Mono': [
    { url: plexMono400, weight: '400' },
    { url: plexMono500, weight: '500' },
    { url: plexMono600, weight: '600' },
  ],
  'Commit Mono': [
    { url: commitMono400, weight: '400' },
    { url: commitMono700, weight: '700' },
  ],
  'Maple Mono': [
    { url: maple400, weight: '400' },
    { url: maple700, weight: '700' },
  ],
  'Monaspace Neon': [
    { url: monaspace400, weight: '400' },
    { url: monaspace500, weight: '500' },
    { url: monaspace600, weight: '600' },
  ],
};

export interface FontPack {
  id: string;
  name: string;
  ui: string;
  mono: string;
  terminal: string;
  /** 终端行高 */
  lineHeight: number;
  ligatures: boolean;
  /** 可选的中文增强包（M3 实现下载），先只展示 */
  addon?: string;
}

export const FONT_PACKS: FontPack[] = [
  { id: 'modern', name: '现代', ui: 'Geist', mono: 'Geist Mono', terminal: 'Maple Mono', lineHeight: 1.25, ligatures: true, addon: 'Maple Mono CN' },
  { id: 'classic', name: '经典', ui: 'Inter', mono: 'JetBrains Mono', terminal: 'JetBrains Mono', lineHeight: 1.2, ligatures: false },
  { id: 'plex', name: '工程', ui: 'IBM Plex Sans', mono: 'IBM Plex Mono', terminal: 'IBM Plex Mono', lineHeight: 1.2, ligatures: false, addon: '思源黑体' },
  { id: 'soft', name: '温润', ui: 'Manrope', mono: 'Commit Mono', terminal: 'Maple Mono', lineHeight: 1.3, ligatures: true, addon: 'Maple Mono CN' },
  { id: 'github', name: 'GitHub 风', ui: 'Instrument Sans', mono: 'Monaspace Neon', terminal: 'Monaspace Neon', lineHeight: 1.25, ligatures: true },
  { id: 'character', name: '个性', ui: 'Space Grotesk', mono: 'Martian Mono', terminal: 'Cascadia Code', lineHeight: 1.2, ligatures: true, addon: '霞鹜文楷' },
];

export const DEFAULT_PACK = 'modern';

/** 系统中文字体回退：macOS / iOS 苹方，Windows 微软雅黑。 */
const CJK = `'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei UI', 'Microsoft YaHei'`;

const loading = new Map<string, Promise<void>>();

/** 注册并加载一个内置字体家族；重复调用只加载一次。 */
export function ensureFamily(family: string): Promise<void> {
  const faces = FACES[family];
  if (!faces) return Promise.resolve();
  let p = loading.get(family);
  if (!p) {
    p = Promise.all(
      faces.map((f) => {
        const face = new FontFace(family, `url(${f.url}) format('woff2')`, {
          weight: f.weight,
          style: 'normal',
          display: 'block',
        });
        document.fonts.add(face);
        return face.load();
      }),
    ).then(() => undefined);
    loading.set(family, p);
  }
  return p;
}

export function findPack(id: string): FontPack {
  return FONT_PACKS.find((p) => p.id === id) ?? FONT_PACKS[0];
}

export function familyStack(family: string, generic: 'sans-serif' | 'monospace'): string {
  return `'${family}', ${CJK}, ${generic === 'monospace' ? 'ui-monospace, ' : 'system-ui, '}${generic}`;
}

/** 加载字体包需要的字体，并写入 CSS 变量。终端字体由终端组件单独应用。 */
export async function applyFontPack(id: string): Promise<FontPack> {
  const pack = findPack(id);
  await Promise.all([ensureFamily(pack.ui), ensureFamily(pack.mono), ensureFamily(pack.terminal)]);
  const root = document.documentElement.style;
  root.setProperty('--f-ui', familyStack(pack.ui, 'sans-serif'));
  root.setProperty('--f-mono', familyStack(pack.mono, 'monospace'));
  root.setProperty('--f-term', familyStack(pack.terminal, 'monospace'));
  return pack;
}
