// 跟踪终端里“正在输入的这一行命令”，供命令联想使用。
//
// 原理：用户开始输入时记下光标位置（提示符的末尾）和提示符文字；之后以屏幕内容为准读出
// 从这个位置到光标之间的文字。只敲了可打印字符和退格时，还会核对屏幕上的文字和键入的完全一致——
// 不回显的输入（密码）对不上，既不会给出联想，也不会被记进历史。
// 全屏程序（vim、top 等使用备用屏幕）里不工作。
import type { IBuffer, IMarker, Terminal } from '@xterm/xterm';

export type LineRead =
  /** 读到了：text 是光标前的命令；atEnd 表示光标在行尾（后面是空白或 zsh/fish 的灰色预测文字）；
      startX / startY 是输入开始处的列和（相对可视区域的）行 */
  | { text: string; atEnd: boolean; startX: number; startY: number }
  /** 回显还没到（刚敲的字还没出现在屏幕上），先保持现状 */
  | 'pending'
  | null;

const PASTE_START = '\x1b[200~';
const PASTE_END = '\x1b[201~';
/** zsh-autosuggestions、fish 等的“预测文字”常用的灰色 */
const GHOST_COLORS = new Set([8, 239, 240, 241, 242, 243, 244, 245, 246]);
const MAX_ROWS = 8;

interface Anchor {
  marker: IMarker | undefined;
  /** 输入开始的列 */
  x: number;
  /** 输入开始的行相对于所在逻辑行（含自动换行）第一行的偏移 */
  rowOff: number;
  /** 提示符：从逻辑行开头到输入开始处的文字 */
  prompt: string;
}

export class InputLine {
  private anchor: Anchor | null = null;
  /** 当前行的起点已不可知（多行粘贴等），等下一次回车 / Ctrl+C 再重新开始 */
  private lost = false;
  /** 回车时回显还没到：等屏幕上出现这条命令再记录 */
  private verify: { marker: IMarker; x: number; text: string; until: number } | null = null;
  /** 这一行里键入的文字 */
  typed = '';
  /** typed 是否就是这一行的全部内容（用过方向键、Tab 等编辑键后只能以屏幕为准） */
  exact = true;

  constructor(
    private term: Terminal,
    private oncommit: (command: string) => void,
  ) {
    term.buffer.onBufferChange(() => this.reset());
  }

  reset(lost = false) {
    this.anchor?.marker?.dispose();
    this.anchor = null;
    this.typed = '';
    this.exact = true;
    this.lost = lost;
  }

  /** 处理一段发往远端的输入（键盘、粘贴、代码片段、分屏同步输入）。 */
  input(data: string) {
    const b = this.term.buffer.active;
    if (b.type !== 'normal') {
      this.reset();
      return;
    }
    if (data === '\r') {
      this.commit();
      return;
    }
    let d = data;
    if (d.startsWith(PASTE_START)) d = d.slice(PASTE_START.length);
    if (d.endsWith(PASTE_END)) d = d.slice(0, -PASTE_END.length);
    if (/[\r\n]/.test(d)) {
      // 带回车的片段 / 多行粘贴：以回车结尾时已经执行，回到新的提示符；否则最后一行还留在输入行里
      this.reset(!/[\r\n]$/.test(d));
      return;
    }
    this.start();
    if (!this.anchor) return;
    for (const ch of d) {
      const c = ch.codePointAt(0)!;
      if (c === 0x1b) {
        // 方向键、Alt+键、功能键：之后以屏幕为准
        this.exact = false;
        break;
      }
      if (c === 0x7f || c === 0x08) this.typed = Array.from(this.typed).slice(0, -1).join('');
      else if (c === 0x03) {
        this.reset(); // Ctrl+C：放弃这一行，下一个提示符重新开始
        return;
      } else if (c === 0x15) this.typed = ''; // Ctrl+U
      else if (c < 0x20) this.exact = false; // Tab、Ctrl+W/A/E/K/R/L 等
      else this.typed += ch;
    }
  }

  /** 屏幕有新内容时调用：核对回车时还没回显的命令 */
  flush() {
    const v = this.verify;
    if (!v) return;
    const b = this.term.buffer.active;
    if (v.marker.isDisposed || Date.now() > v.until) {
      this.dropVerify();
      return;
    }
    if (b.type !== 'normal') return;
    let y = v.marker.line;
    let s = b.getLine(y)?.translateToString(false, v.x) ?? '';
    while (y - v.marker.line < MAX_ROWS && b.getLine(y + 1)?.isWrapped) {
      y++;
      s += b.getLine(y)?.translateToString(false) ?? '';
    }
    if (s.startsWith(v.text)) {
      // 命令后面只能是空白，或隔开一段的右侧提示符（zsh RPROMPT）
      const rest = s.slice(v.text.length);
      if (!rest.trim() || /^ {2,}/.test(rest)) {
        this.oncommit(v.text);
        this.dropVerify();
        return;
      }
    }
    // 光标已经离开这一行（远端处理了回车）：这一行不会再变，对不上就放弃（例如不回显的密码）
    if (b.baseY + b.cursorY > y) this.dropVerify();
  }

  /** 读出光标前的命令。 */
  read(retry = true): LineRead {
    const a = this.anchor;
    const b = this.term.buffer.active;
    if (!a || this.lost || b.type !== 'normal') return null;
    const cols = this.term.cols;
    const cy = b.baseY + b.cursorY;
    const cx = Math.min(b.cursorX, cols);
    let y0 = cy;
    while (y0 > 0 && cy - y0 <= MAX_ROWS && b.getLine(y0)?.isWrapped) y0--;
    const ay = y0 + a.rowOff;
    if (ay > cy || cy - ay > MAX_ROWS || (ay === cy && cx < a.x)) return null;
    // 提示符必须还是原来那个（Ctrl+R 搜索、程序输出等会改掉它）；提示符为空时要求还在原来那一行
    if (this.promptAt(b, y0, ay, a.x) !== a.prompt) return null;
    if (!a.prompt.trim() && a.marker?.line !== ay) return null;
    let text = '';
    for (let y = ay; y <= cy; y++) {
      text += b.getLine(y)?.translateToString(false, y === ay ? a.x : 0, y === cy ? cx : cols) ?? '';
    }
    if (this.exact && text !== this.typed) {
      if (this.typed.startsWith(text) || text.startsWith(this.typed)) return 'pending';
      // 在提示符出来之前就开始输入了：键入的文字出现在新提示符之后，改从那里开始
      if (retry && this.typed && ay === cy && /^[\x20-\x7e]+$/.test(this.typed) && text.endsWith(this.typed)) {
        this.setAnchor(b, cy, cx - this.typed.length);
        return this.read(false);
      }
      return null;
    }
    const atEnd = this.blank(b, cy, cx) && this.blank(b, cy, cx + 1);
    return { text, atEnd, startX: a.x, startY: ay - b.baseY };
  }

  /** 正在跟踪一行输入（可能需要随屏幕更新联想） */
  get tracking() {
    return !!this.anchor && !this.lost;
  }

  dispose() {
    this.reset();
    this.dropVerify();
  }

  // ---------------------------------------------------------------- 内部

  private start() {
    if (this.anchor || this.lost) return;
    const b = this.term.buffer.active;
    this.setAnchor(b, b.baseY + b.cursorY, Math.min(b.cursorX, this.term.cols));
    this.typed = '';
    this.exact = true;
  }

  private setAnchor(b: IBuffer, y: number, x: number) {
    this.anchor?.marker?.dispose();
    let y0 = y;
    while (y0 > 0 && y - y0 <= MAX_ROWS && b.getLine(y0)?.isWrapped) y0--;
    this.anchor = {
      marker: this.term.registerMarker(y - (b.baseY + b.cursorY)),
      x,
      rowOff: y - y0,
      prompt: this.promptAt(b, y0, y, x),
    };
  }

  private promptAt(b: IBuffer, y0: number, ay: number, ax: number) {
    let p = '';
    for (let y = y0; y <= ay; y++) p += b.getLine(y)?.translateToString(false, 0, y === ay ? ax : this.term.cols) ?? '';
    return p;
  }

  /** 这一格是空白，或是 zsh/fish 的灰色预测文字 */
  private blank(b: IBuffer, y: number, x: number) {
    if (x >= this.term.cols) return true;
    const cell = b.getLine(y)?.getCell(x);
    if (!cell) return true;
    const ch = cell.getChars();
    if (!ch || ch === ' ') return true;
    return !!cell.isDim() || (cell.isFgPalette() && GHOST_COLORS.has(cell.getFgColor()));
  }

  private commit() {
    const r = this.read();
    if (r && r !== 'pending') {
      if (r.atEnd) this.oncommit(r.text);
    } else if (r === 'pending' && this.typed.trim() && this.anchor?.marker && !this.anchor.marker.isDisposed) {
      // 敲得比回显快：等屏幕上出现这条命令再记录（没有回显的密码永远等不到）
      this.dropVerify();
      this.verify = { marker: this.anchor.marker, x: this.anchor.x, text: this.typed, until: Date.now() + 3000 };
      this.anchor.marker = undefined; // 交给 verify 释放
    }
    this.reset();
  }

  private dropVerify() {
    this.verify?.marker.dispose();
    this.verify = null;
  }
}
