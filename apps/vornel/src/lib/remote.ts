// RDP / VNC 远程桌面的前后端接口。
import { Channel, invoke } from '@tauri-apps/api/core';

export type RemoteEvent =
  | { type: 'resize'; width: number; height: number }
  | { type: 'connected' }
  | { type: 'closed'; reason: string | null };

export type RemoteInput =
  | { type: 'pointer'; x: number; y: number; button: number; down: boolean; move: boolean }
  | { type: 'wheel'; x: number; y: number; delta: number }
  | { type: 'key'; code: string; key: string; down: boolean };

export interface RemoteOpenArgs {
  hostId: string;
  password?: string;
  savePassword: boolean;
  width: number;
  height: number;
  /** 一个脏矩形：8 字节头（x, y, w, h，各 u16 小端）+ RGBA 像素 */
  onFrame: (buf: ArrayBuffer) => void;
  onEvent: (ev: RemoteEvent) => void;
}

export function remoteOpen(a: RemoteOpenArgs): Promise<string> {
  const onFrame = new Channel<ArrayBuffer | number[]>();
  onFrame.onmessage = (m) => a.onFrame(m instanceof ArrayBuffer ? m : Uint8Array.from(m).buffer);
  const onEvent = new Channel<RemoteEvent>();
  onEvent.onmessage = a.onEvent;
  return invoke<string>('remote_open', {
    hostId: a.hostId,
    password: a.password ?? null,
    savePassword: a.savePassword,
    width: a.width,
    height: a.height,
    onFrame,
    onEvent,
  });
}

export const remoteInput = (sessionId: string, input: RemoteInput) =>
  invoke<void>('remote_input', { sessionId, input });
export const remoteClose = (sessionId: string) => invoke<void>('remote_close', { sessionId });
