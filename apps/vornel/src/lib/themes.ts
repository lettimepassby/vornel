import type { ITheme } from '@xterm/xterm';

export type ThemeId = 'ink' | 'paper';

/** 终端 ANSI 配色，与 DESIGN.md 3.2 一致。 */
export const TERMINAL_THEMES: Record<ThemeId, ITheme> = {
  ink: {
    background: '#0B0B0D',
    foreground: '#D7D7DC',
    cursor: '#8EA2FF',
    cursorAccent: '#0B0B0D',
    selectionBackground: 'rgba(142,162,255,0.28)',
    black: '#1A1A1E',
    red: '#F7768E',
    green: '#9ECE6A',
    yellow: '#E0AF68',
    blue: '#7AA2F7',
    magenta: '#BB9AF7',
    cyan: '#7DCFFF',
    white: '#C8C8D0',
    brightBlack: '#62626C',
    brightRed: '#FF8FA3',
    brightGreen: '#B5E08A',
    brightYellow: '#F0C483',
    brightBlue: '#98B8FF',
    brightMagenta: '#CDB2FF',
    brightCyan: '#9ADBFF',
    brightWhite: '#F2F2F5',
  },
  paper: {
    background: '#FFFFFF',
    foreground: '#2A2925',
    cursor: '#4353D1',
    cursorAccent: '#FFFFFF',
    selectionBackground: 'rgba(67,83,209,0.18)',
    black: '#2A2925',
    red: '#C2334D',
    green: '#3F7D20',
    yellow: '#9A6700',
    blue: '#3557C9',
    magenta: '#8E44AD',
    cyan: '#0E7490',
    white: '#625F59',
    brightBlack: '#8F8B84',
    brightRed: '#D9465F',
    brightGreen: '#4E9A2A',
    brightYellow: '#B07A00',
    brightBlue: '#4A6BDB',
    brightMagenta: '#A35BC2',
    brightCyan: '#1689A8',
    brightWhite: '#1C1B19',
  },
};

/** 终端里由应用写入的内联徽标（SSH / KEY / FWD / ERR），见 DESIGN.md 第 6 节。 */
const BADGE_RGB: Record<ThemeId, Record<BadgeKind, [number, number, number]>> = {
  ink: { ssh: [142, 162, 255], key: [158, 206, 106], fwd: [224, 175, 104], err: [247, 118, 142], info: [154, 154, 163] },
  paper: { ssh: [67, 83, 209], key: [63, 125, 32], fwd: [154, 103, 0], err: [194, 51, 77], info: [98, 95, 89] },
};

export type BadgeKind = 'ssh' | 'key' | 'fwd' | 'err' | 'info';

export function badge(theme: ThemeId, kind: BadgeKind, label: string, text: string): string {
  const [r, g, b] = BADGE_RGB[theme][kind];
  const bg = TERMINAL_THEMES[theme].background as string;
  const [br, bgG, bb] = [1, 3, 5].map((i) => parseInt(bg.slice(i, i + 2), 16));
  // 15% 同色底：手动与背景混色，避免依赖终端的透明度支持
  const mix = (c: number, base: number) => Math.round(base + (c - base) * 0.15);
  const badgeBg = `\x1b[48;2;${mix(r, br)};${mix(g, bgG)};${mix(b, bb)}m`;
  const badgeFg = `\x1b[1;38;2;${r};${g};${b}m`;
  const pad = label.padEnd(4, ' ');
  return `${badgeBg}${badgeFg} ${pad}\x1b[0m  ${text}\r\n`;
}
