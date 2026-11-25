// 命令联想：从命令历史（本机加密保存）、代码片段和内置常用命令里找出以当前输入开头的命令。
import * as api from './api';
import type { HistoryEntry } from './api';
import { COMMON_COMMANDS } from './commands';
import { data, prefs } from './store.svelte';

export type SuggestSource = 'history' | 'snippet' | 'common';
export interface Suggestion {
  text: string;
  source: SuggestSource;
  /** 代码片段的名称 */
  label?: string;
}

// 与 crates/vornel-core/src/history.rs 保持一致
const HISTORY_LIMIT = 3000;
const COMMAND_MAX = 500;

let entries: HistoryEntry[] = [];
let loaded: Promise<void> | null = null;

/** 读取命令历史（只读一次；失败时下次再试） */
export function loadHistory() {
  loaded ??= api
    .historyList()
    .then((l) => {
      // 读取期间新记的命令保留下来
      const seen = new Set(l.map((e) => `${e.hostId}\n${e.command}`));
      entries = [...l, ...entries.filter((e) => !seen.has(`${e.hostId}\n${e.command}`))];
    })
    .catch(() => {
      loaded = null;
    });
  return loaded;
}

function normalize(command: string): string | null {
  if (/^[ \t]/.test(command)) return null; // 以空格开头：不记录（同 bash 的 ignorespace）
  const c = command.trimEnd();
  // eslint-disable-next-line no-control-regex
  if (!c || Array.from(c).length > COMMAND_MAX || /[\u0000-\u001f\u007f-\u009f]/.test(c)) return null;
  return c;
}

/** 记下一条执行过的命令 */
export function recordCommand(hostId: string | null, command: string) {
  const c = normalize(command);
  if (!c) return;
  const now = Date.now();
  const e = entries.find((x) => x.hostId === hostId && x.command === c);
  if (e) {
    e.count += 1;
    e.lastUsed = now;
  } else {
    entries.push({ command: c, hostId, count: 1, lastUsed: now });
  }
  if (entries.length > HISTORY_LIMIT) {
    entries.sort((a, b) => b.lastUsed - a.lastUsed);
    entries.length = HISTORY_LIMIT;
  }
  void api.historyAdd(hostId, c).catch(() => {});
}

/** 从历史里删掉一条（联想列表里点 ×） */
export function forgetCommand(command: string) {
  entries = entries.filter((e) => e.command !== command);
  void api.historyRemove(command).catch(() => {});
}

export async function clearHistory() {
  entries = [];
  await api.historyClear();
}

/** 历史条数（设置里显示） */
export async function historyCount() {
  await loadHistory();
  return entries.length;
}

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

/** 以 prefix 开头的建议，按“同一台主机 > 常用 > 最近”排序，再接代码片段和内置常用命令。 */
export function suggest(prefix: string, hostId: string | null, limit = 6): Suggestion[] {
  if (!prefix.trim() || prefix.length > COMMAND_MAX) return [];
  const out: Suggestion[] = [];
  const seen = new Set<string>();
  const add = (s: Suggestion) => {
    if (out.length >= limit || seen.has(s.text) || s.text.length <= prefix.length || !s.text.startsWith(prefix)) return;
    seen.add(s.text);
    out.push(s);
  };

  if (prefs.commandHistory) {
    const now = Date.now();
    const score = new Map<string, number>();
    for (const e of entries) {
      if (e.command.length <= prefix.length || !e.command.startsWith(prefix)) continue;
      const age = now - e.lastUsed;
      const recency = age < HOUR ? 4 : age < DAY ? 2 : age < 7 * DAY ? 1 : 0.5;
      const s = Math.log2(e.count + 1) * recency * (e.hostId === hostId ? 3 : 1);
      score.set(e.command, Math.max(score.get(e.command) ?? 0, s));
    }
    for (const [text] of [...score].sort((a, b) => b[1] - a[1])) add({ text, source: 'history' });
  }
  for (const s of data.snippets) {
    if (!s.command.includes('\n')) add({ text: s.command, source: 'snippet', label: s.name });
  }
  // 内置命令至少敲两个字符再出来，避免一敲就弹
  if (prefix.length >= 2) for (const c of COMMON_COMMANDS) add({ text: c, source: 'common' });
  return out;
}
