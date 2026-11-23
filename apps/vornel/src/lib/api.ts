import { Channel, invoke } from '@tauri-apps/api/core';

export type Protocol = 'ssh' | 'sftp' | 'rdp' | 'telnet';

export type AuthKind = { type: 'password' } | { type: 'keyFile'; path: string };

export interface Host {
  id: string;
  name: string;
  address: string;
  port: number;
  username: string;
  group: string | null;
  protocol: Protocol;
  auth: AuthKind;
}

export type SessionEvent =
  | { type: 'connected'; fingerprint: string; newHostKey: boolean }
  | { type: 'closed'; exitStatus: number | null };

export const hostsList = () => invoke<Host[]>('hosts_list');
export const hostsSave = (host: Host) => invoke<Host>('hosts_save', { host });
export const hostsDelete = (id: string) => invoke<void>('hosts_delete', { id });

export interface ConnectArgs {
  hostId: string;
  password?: string;
  passphrase?: string;
  cols: number;
  rows: number;
  onData: (bytes: Uint8Array) => void;
  onEvent: (event: SessionEvent) => void;
}

/** 建立 SSH 会话，返回会话 id。终端输出走二进制通道，避免 JSON 数组的开销。 */
export async function sshConnect(args: ConnectArgs): Promise<string> {
  const onData = new Channel<ArrayBuffer | number[]>();
  onData.onmessage = (msg) => {
    args.onData(msg instanceof ArrayBuffer ? new Uint8Array(msg) : Uint8Array.from(msg));
  };
  const onEvent = new Channel<SessionEvent>();
  onEvent.onmessage = args.onEvent;

  return invoke<string>('ssh_connect', {
    hostId: args.hostId,
    password: args.password ?? null,
    passphrase: args.passphrase ?? null,
    cols: args.cols,
    rows: args.rows,
    onData,
    onEvent,
  });
}

export const sshWrite = (sessionId: string, data: string) =>
  invoke<void>('ssh_write', { sessionId, data });
export const sshResize = (sessionId: string, cols: number, rows: number) =>
  invoke<void>('ssh_resize', { sessionId, cols, rows });
export const sshClose = (sessionId: string) => invoke<void>('ssh_close', { sessionId });

/** 新建主机时的默认值。 */
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
  };
}

/** Tauri 命令返回的错误是字符串；其他异常取 message。 */
export function errorText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
