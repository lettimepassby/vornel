// 全局数据：从 Vault 读取，界面各处共享。
import * as api from './api';
import type { Forward, Host, Key, KnownHost, Prefs, Snippet } from './api';
import { DEFAULT_PACK } from './fonts';

const PREFS_CACHE = 'vornel.prefs';

const DEFAULT_PREFS: Prefs = {
  id: '00000000-0000-0000-0000-000000000000',
  theme: 'ink',
  fontPack: DEFAULT_PACK,
  terminalFontSize: 13,
  fontOverrides: {},
  syncSecrets: false,
  screenReader: false,
};

function cachedPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(PREFS_CACHE);
    if (raw) return { ...DEFAULT_PREFS, ...JSON.parse(raw) };
  } catch {
    // 读取失败用默认值
  }
  return { ...DEFAULT_PREFS };
}

export const data = $state({
  ready: false,
  vaultError: null as string | null,
  hosts: [] as Host[],
  keys: [] as Key[],
  snippets: [] as Snippet[],
  forwards: [] as Forward[],
  knownHosts: [] as KnownHost[],
  runningForwards: [] as string[],
});

/** 偏好设置：先用本机缓存（保证启动时主题立即生效），Vault 读到后再覆盖。 */
export const prefs = $state<Prefs>(cachedPrefs());

export async function loadAll() {
  const status = await api.vaultStatus();
  data.vaultError = status.ok ? null : status.error;
  if (!status.ok) {
    data.ready = true;
    return;
  }
  const [hosts, keys, snippets, forwards, knownHosts, prefsList, running] = await Promise.all([
    api.list<Host>('host'),
    api.list<Key>('key'),
    api.list<Snippet>('snippet'),
    api.list<Forward>('forward'),
    api.list<KnownHost>('known_host'),
    api.list<Prefs>('prefs'),
    api.forwardRunning(),
  ]);
  const byName = <T extends { name: string }>(a: T, b: T) => a.name.localeCompare(b.name);
  data.hosts = hosts.sort(byName);
  data.keys = keys.sort(byName);
  data.snippets = snippets.sort(byName);
  data.forwards = forwards.sort(byName);
  data.knownHosts = knownHosts.sort((a, b) => a.host.localeCompare(b.host));
  data.runningForwards = running;
  if (prefsList[0]) Object.assign(prefs, prefsList[0]);
  data.ready = true;
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;

/** 偏好变化后写回 Vault（防抖），并更新本机缓存。 */
export function persistPrefs() {
  try {
    localStorage.setItem(PREFS_CACHE, JSON.stringify(prefs));
  } catch {
    // 忽略
  }
  if (!data.ready || data.vaultError) return;
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    void api.save('prefs', $state.snapshot(prefs));
  }, 400);
}

export function hostById(id: string | null | undefined): Host | undefined {
  return id ? data.hosts.find((h) => h.id === id) : undefined;
}
