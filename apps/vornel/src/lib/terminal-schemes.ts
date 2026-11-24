// 终端配色方案。“跟随界面”使用 DESIGN.md 3.2 的 Ink / Paper；其余为各项目公开的标准配色。
import type { ITheme } from '@xterm/xterm';
import { TERMINAL_THEMES, type ThemeId } from './themes';

export interface Scheme {
  id: string;
  name: string;
  /** 深色背景（决定应用内徽标用哪套颜色） */
  dark: boolean;
  theme: ITheme;
}

type Ansi = [string, string, string, string, string, string, string, string];

function scheme(id: string, name: string, dark: boolean, bg: string, fg: string, cursor: string, sel: string, n: Ansi, b: Ansi): Scheme {
  return {
    id,
    name,
    dark,
    theme: {
      background: bg,
      foreground: fg,
      cursor,
      cursorAccent: bg,
      selectionBackground: sel,
      black: n[0],
      red: n[1],
      green: n[2],
      yellow: n[3],
      blue: n[4],
      magenta: n[5],
      cyan: n[6],
      white: n[7],
      brightBlack: b[0],
      brightRed: b[1],
      brightGreen: b[2],
      brightYellow: b[3],
      brightBlue: b[4],
      brightMagenta: b[5],
      brightCyan: b[6],
      brightWhite: b[7],
    },
  };
}

export const SCHEMES: Scheme[] = [
  scheme('dracula', 'Dracula', true, '#282A36', '#F8F8F2', '#F8F8F2', 'rgba(68,71,90,0.8)',
    ['#21222C', '#FF5555', '#50FA7B', '#F1FA8C', '#BD93F9', '#FF79C6', '#8BE9FD', '#F8F8F2'],
    ['#6272A4', '#FF6E6E', '#69FF94', '#FFFFA5', '#D6ACFF', '#FF92DF', '#A4FFFF', '#FFFFFF']),
  scheme('nord', 'Nord', true, '#2E3440', '#D8DEE9', '#D8DEE9', 'rgba(67,76,94,0.8)',
    ['#3B4252', '#BF616A', '#A3BE8C', '#EBCB8B', '#81A1C1', '#B48EAD', '#88C0D0', '#E5E9F0'],
    ['#4C566A', '#BF616A', '#A3BE8C', '#EBCB8B', '#81A1C1', '#B48EAD', '#8FBCBB', '#ECEFF4']),
  scheme('one-dark', 'One Dark', true, '#282C34', '#ABB2BF', '#528BFF', 'rgba(62,68,81,0.8)',
    ['#282C34', '#E06C75', '#98C379', '#E5C07B', '#61AFEF', '#C678DD', '#56B6C2', '#ABB2BF'],
    ['#5C6370', '#E06C75', '#98C379', '#E5C07B', '#61AFEF', '#C678DD', '#56B6C2', '#FFFFFF']),
  scheme('tokyo-night', 'Tokyo Night', true, '#1A1B26', '#C0CAF5', '#C0CAF5', 'rgba(40,52,87,0.8)',
    ['#15161E', '#F7768E', '#9ECE6A', '#E0AF68', '#7AA2F7', '#BB9AF7', '#7DCFFF', '#A9B1D6'],
    ['#414868', '#F7768E', '#9ECE6A', '#E0AF68', '#7AA2F7', '#BB9AF7', '#7DCFFF', '#C0CAF5']),
  scheme('gruvbox-dark', 'Gruvbox Dark', true, '#282828', '#EBDBB2', '#EBDBB2', 'rgba(80,73,69,0.8)',
    ['#282828', '#CC241D', '#98971A', '#D79921', '#458588', '#B16286', '#689D6A', '#A89984'],
    ['#928374', '#FB4934', '#B8BB26', '#FABD2F', '#83A598', '#D3869B', '#8EC07C', '#EBDBB2']),
  scheme('catppuccin-mocha', 'Catppuccin Mocha', true, '#1E1E2E', '#CDD6F4', '#F5E0DC', 'rgba(88,91,112,0.6)',
    ['#45475A', '#F38BA8', '#A6E3A1', '#F9E2AF', '#89B4FA', '#F5C2E7', '#94E2D5', '#BAC2DE'],
    ['#585B70', '#F38BA8', '#A6E3A1', '#F9E2AF', '#89B4FA', '#F5C2E7', '#94E2D5', '#A6ADC8']),
  scheme('monokai', 'Monokai', true, '#272822', '#F8F8F2', '#F8F8F0', 'rgba(73,72,62,0.9)',
    ['#272822', '#F92672', '#A6E22E', '#F4BF75', '#66D9EF', '#AE81FF', '#A1EFE4', '#F8F8F2'],
    ['#75715E', '#F92672', '#A6E22E', '#F4BF75', '#66D9EF', '#AE81FF', '#A1EFE4', '#F9F8F5']),
  scheme('solarized-dark', 'Solarized Dark', true, '#002B36', '#839496', '#93A1A1', 'rgba(7,54,66,0.9)',
    ['#073642', '#DC322F', '#859900', '#B58900', '#268BD2', '#D33682', '#2AA198', '#EEE8D5'],
    ['#002B36', '#CB4B16', '#586E75', '#657B83', '#839496', '#6C71C4', '#93A1A1', '#FDF6E3']),
  scheme('solarized-light', 'Solarized Light', false, '#FDF6E3', '#657B83', '#586E75', 'rgba(238,232,213,0.9)',
    ['#073642', '#DC322F', '#859900', '#B58900', '#268BD2', '#D33682', '#2AA198', '#EEE8D5'],
    ['#002B36', '#CB4B16', '#586E75', '#657B83', '#839496', '#6C71C4', '#93A1A1', '#FDF6E3']),
  scheme('github-light', 'GitHub Light', false, '#FFFFFF', '#24292F', '#0969DA', 'rgba(9,105,218,0.18)',
    ['#24292F', '#CF222E', '#116329', '#4D2D00', '#0969DA', '#8250DF', '#1B7C83', '#6E7781'],
    ['#57606A', '#A40E26', '#1A7F37', '#633C01', '#218BFF', '#A475F9', '#3192AA', '#8C959F']),
];

/** 当前生效的终端配色：`auto` 跟随界面主题 */
export function resolveScheme(id: string | undefined, uiTheme: ThemeId): { theme: ITheme; dark: boolean } {
  const s = SCHEMES.find((x) => x.id === id);
  if (s) return { theme: s.theme, dark: s.dark };
  return { theme: TERMINAL_THEMES[uiTheme], dark: uiTheme === 'ink' };
}
