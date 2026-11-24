import { Channel, invoke } from '@tauri-apps/api/core';

// ---------------------------------------------------------------- 数据模型（与 vornel-core 一致）

export type Protocol = 'ssh' | 'mosh' | 'telnet' | 'serial' | 'rdp' | 'vnc';

export type AuthKind =
  | { type: 'password' }
  | { type: 'key'; keyId: string }
  | { type: 'keyFile'; path: string }
  | { type: 'agent' };

export interface SerialConfig {
  baudRate: number;
  dataBits: number;
  parity: 'none' | 'odd' | 'even';
  stopBits: number;
  flowControl: 'none' | 'software' | 'hardware';
}

export interface Host {
  id: string;
  name: string;
  address: string;
  port: number;
  username: string;
  group: string | null;
  protocol: Protocol;
  auth: AuthKind;
  jumpHost: string | null;
  agentForward: boolean;
  savePassword: boolean;
  serial: SerialConfig | null;
  domain: string | null;
  /** 远端系统（连接后自动识别），如 ubuntu / debian / macos */
  os?: string | null;
  /** 最近一次连接成功的时间（毫秒） */
  lastConnected?: number | null;
}

export interface Key {
  id: string;
  name: string;
  algorithm: string;
  publicKey: string;
  /** 永远为空：私钥不离开 Rust 层 */
  privateKey: string;
  encrypted: boolean;
  fingerprint: string;
  sync: boolean;
}

export interface Snippet {
  id: string;
  name: string;
  command: string;
  run: boolean;
}

export type ForwardKind = 'local' | 'remote' | 'dynamic';

export interface Forward {
  id: string;
  name: string;
  hostId: string;
  kind: ForwardKind;
  bindAddress: string;
  bindPort: number;
  targetHost: string;
  targetPort: number;
}

export interface KnownHost {
  id: string;
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
  publicKey: string;
  addedAt: number;
}

export interface Prefs {
  id: string;
  theme: 'ink' | 'paper';
  fontPack: string;
  terminalFontSize: number;
  fontOverrides: Record<string, string>;
  syncSecrets: boolean;
  screenReader: boolean;
  copyOnSelect: boolean;
  /** 分组外观：分组名 → 样式 */
  groupStyles: Record<string, GroupStyle>;
  /** 终端配色：auto 跟随界面 */
  terminalTheme: string;
  /** 终端行高；null 使用字体包默认 */
  terminalLineHeight: number | null;
  cursorStyle: 'block' | 'bar' | 'underline';
  cursorBlink: boolean;
  /** SSH 终端底部显示服务器资源栏 */
  showStats: boolean;
}

export interface GroupStyle {
  /** `os:ubuntu` 或 `icon:cloud` */
  icon?: string | null;
  color?: string | null;
  /** 自定义图片（PNG data URL） */
  image?: string | null;
}

export const PROTOCOL_PORTS: Record<Protocol, number> = {
  ssh: 22,
  mosh: 22,
  telnet: 23,
  serial: 0,
  rdp: 3389,
  vnc: 5900,
};

export const PROTOCOL_LABELS: Record<Protocol, string> = {
  ssh: 'SSH',
  mosh: 'Mosh',
  telnet: 'Telnet',
  serial: '串口',
  rdp: 'RDP',
  vnc: 'VNC',
};

// ---------------------------------------------------------------- Vault 记录

type Kind = 'host' | 'key' | 'snippet' | 'forward' | 'known_host' | 'prefs' | 'secret';

export const vaultStatus = () => invoke<{ ok: boolean; error: string | null }>('vault_status');
export const vaultReset = () => invoke<void>('vault_reset');
export const list = <T>(kind: Kind) => invoke<T[]>('records_list', { kind });
/** 读取主机保存的密码（用于在编辑主机时查看） */
/** 记录主机刚刚连接成功（主机列表按最近连接排序） */
export const hostTouch = (hostId: string) => invoke<number>('host_touch', { hostId });

export const secretGet = (hostId: string) => invoke<string | null>('secret_get', { hostId });

export const save = (kind: Exclude<Kind, 'known_host'>, value: unknown) =>
  invoke<void>('record_save', { kind, value });
export const remove = (kind: Kind | 'secret', id: string) => invoke<void>('record_delete', { kind, id });

export const keyGenerate = (name: string) => invoke<Key>('key_generate', { name });
export const keyImport = (name: string, pem: string, passphrase: string) =>
  invoke<Key>('key_import', { name, pem, passphrase: passphrase || null });

// ---------------------------------------------------------------- 终端会话

export type SessionEvent =
  | { type: 'hostKey'; host: string; port: number; algorithm: string; fingerprint: string; new: boolean }
  | { type: 'connected' }
  | { type: 'os'; os: string }
  | { type: 'closed'; exitStatus: number | null; reason: string | null };

export interface OpenArgs {
  hostId: string;
  password?: string;
  passphrase?: string;
  savePassword?: boolean;
  cols: number;
  rows: number;
  onData: (bytes: Uint8Array) => void;
  onEvent: (event: SessionEvent) => void;
}

function bytesChannel(cb: (b: Uint8Array) => void) {
  const ch = new Channel<ArrayBuffer | number[]>();
  ch.onmessage = (m) => cb(m instanceof ArrayBuffer ? new Uint8Array(m) : Uint8Array.from(m));
  return ch;
}

/** 打开终端会话（SSH / Mosh / Telnet / 串口），返回会话 id。 */
export function sessionOpen(a: OpenArgs): Promise<string> {
  const onEvent = new Channel<SessionEvent>();
  onEvent.onmessage = a.onEvent;
  return invoke<string>('session_open', {
    hostId: a.hostId,
    password: a.password ?? null,
    passphrase: a.passphrase ?? null,
    savePassword: a.savePassword ?? false,
    cols: a.cols,
    rows: a.rows,
    onData: bytesChannel(a.onData),
    onEvent,
  });
}

export const sessionWrite = (sessionId: string, data: string) =>
  invoke<void>('session_write', { sessionId, data });
export const sessionResize = (sessionId: string, cols: number, rows: number) =>
  invoke<void>('session_resize', { sessionId, cols, rows });
export const sessionClose = (sessionId: string) => invoke<void>('session_close', { sessionId });
export const systemFonts = () => invoke<{ name: string; monospace: boolean }[]>('system_fonts');
export const serialPorts = () => invoke<string[]>('serial_ports');
export const hostKeyTrust = (host: string, port: number) => invoke<void>('host_key_trust', { host, port });

// ---------------------------------------------------------------- SFTP

export interface FileEntry {
  name: string;
  isDir: boolean;
  isLink: boolean;
  size: number;
  modified: number | null;
  mode: number | null;
}

export interface Progress {
  done: number;
  total: number;
}

function progressChannel(cb: (p: Progress) => void) {
  const ch = new Channel<Progress>();
  ch.onmessage = cb;
  return ch;
}

export const sftpHome = (sessionId: string) => invoke<string>('sftp_home', { sessionId });
export const sftpList = (sessionId: string, path: string) => invoke<FileEntry[]>('sftp_list', { sessionId, path });
export const sftpMkdir = (sessionId: string, path: string) => invoke<void>('sftp_mkdir', { sessionId, path });
export const sftpRename = (sessionId: string, from: string, to: string) =>
  invoke<void>('sftp_rename', { sessionId, from, to });
export const sftpRemove = (sessionId: string, path: string) => invoke<void>('sftp_remove', { sessionId, path });
export const sftpDownload = (sessionId: string, remote: string, localDir: string, onProgress: (p: Progress) => void) =>
  invoke<string>('sftp_download', { sessionId, remote, localDir, onProgress: progressChannel(onProgress) });
export const sftpUpload = (sessionId: string, local: string, remoteDir: string, onProgress: (p: Progress) => void) =>
  invoke<string>('sftp_upload', { sessionId, local, remoteDir, onProgress: progressChannel(onProgress) });
export const localHome = () => invoke<string>('local_home');
export const localList = (path: string) => invoke<FileEntry[]>('local_list', { path });

// ---------------------------------------------------------------- 端口转发

export const forwardStart = (forwardId: string, password?: string) =>
  invoke<number>('forward_start', { forwardId, password: password ?? null });
export const forwardStop = (forwardId: string) => invoke<void>('forward_stop', { forwardId });
export const forwardRunning = () => invoke<string[]>('forward_running');

// ---------------------------------------------------------------- 同步

export type BackendConfig =
  | { type: 'folder'; path: string }
  | {
      type: 's3';
      endpoint: string;
      bucket: string;
      region: string;
      accessKeyId: string;
      secretAccessKey: string;
      root: string;
    }
  | { type: 'webdav'; endpoint: string; username: string; password: string; root: string }
  | { type: 'gdrive' | 'oneDrive' | 'dropbox'; clientId: string; clientSecret: string; refreshToken: string };

export interface SyncReport {
  pulled: number;
  pushed: number;
  purged: number;
  created: boolean;
  joined: boolean;
}

export const syncGetConfig = () => invoke<BackendConfig | null>('sync_get_config');
export const syncSetConfig = (config: BackendConfig | null) => invoke<void>('sync_set_config', { config });
export const syncCheck = () => invoke<{ configured: boolean; needPassword: boolean | null }>('sync_check');
export const syncNow = (password?: string) => invoke<SyncReport>('sync_now', { password: password ?? null });
export const syncChangePassword = (newPassword: string) => invoke<void>('sync_change_password', { newPassword });
export const syncOAuth = (provider: 'google' | 'microsoft' | 'dropbox', clientId: string, clientSecret: string) =>
  invoke<BackendConfig>('sync_oauth', { provider, clientId, clientSecret });

// ---------------------------------------------------------------- 工具

export function blankHost(): Host {
  return {
    id: crypto.randomUUID(),
    name: '',
    address: '',
    port: 22,
    username: 'root',
    group: null,
    protocol: 'ssh',
    auth: { type: 'password' },
    jumpHost: null,
    agentForward: false,
    savePassword: false,
    serial: null,
    domain: null,
    os: null,
    lastConnected: null,
  };
}

export function errorText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}

export function formatTime(secs: number | null): string {
  if (!secs) return '—';
  const d = new Date(secs * 1000);
  const now = new Date();
  const pad = (x: number) => String(x).padStart(2, '0');
  return d.toDateString() === now.toDateString()
    ? `${pad(d.getHours())}:${pad(d.getMinutes())}`
    : `${d.getFullYear() === now.getFullYear() ? '' : d.getFullYear() + '-'}${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export interface ImportReport {
  hosts: number;
  keys: number;
  passwords: number;
  snippets: number;
  knownHosts: number;
  skipped: number;
  errors: string[];
}
/** 从 Vornel 导入文件（JSON）导入主机、密钥、片段、已知主机 */
export const importFile = (path: string) => invoke<ImportReport>('import_file', { path });

export interface ServerStats {
  cpuTotal: number;
  cpuIdle: number;
  cores: number;
  memTotal: number;
  memAvailable: number;
  swapTotal: number;
  swapFree: number;
  load: [number, number, number];
  netRx: number;
  netTx: number;
  diskTotal: number;
  diskUsed: number;
  uptimeSecs: number;
}
/** 读取一次服务器资源；非 Linux 返回 null */
export const sessionStats = (sessionId: string) => invoke<ServerStats | null>('session_stats', { sessionId });
