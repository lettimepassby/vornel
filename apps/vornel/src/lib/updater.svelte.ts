// 应用内更新（仅桌面端；iOS 走 App Store，没有注册 updater / process 插件）。
// 启动时的提示条和设置里的“检查更新”共用这里的状态。
import { getVersion } from '@tauri-apps/api/app';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';

const isIOS = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);

/** 当前平台是否支持应用内更新 */
export const updaterSupported = !isIOS;

export const updater = $state({
  /** 当前运行的版本 */
  current: '',
  /** 服务器上的新版本；没有新版本时为 null */
  update: null as Update | null,
  checking: false,
  /** 至少完成过一次检查（用来显示“已是最新版本”） */
  checked: false,
  downloading: false,
  /** 下载进度 0–100；服务器没给文件大小时为 null */
  progress: null as number | null,
  error: '',
  /** 用户关掉了顶部提示条 */
  dismissed: false,
});

export async function loadCurrentVersion() {
  if (updater.current) return;
  updater.current = await getVersion().catch(() => '');
}

/** 检查更新。silent 为 true 时出错不提示（启动时的后台检查）。 */
export async function checkForUpdate(silent = false) {
  if (!updaterSupported || updater.checking || updater.downloading) return;
  updater.checking = true;
  if (!silent) updater.error = '';
  try {
    const next = await check({ timeout: 20_000 });
    const prev = updater.update;
    updater.update = next;
    if (prev && prev !== next) void prev.close().catch(() => {});
    updater.checked = true;
  } catch (e) {
    if (!silent) updater.error = `检查更新失败：${String(e)}`;
  } finally {
    updater.checking = false;
  }
}

/** 下载并安装新版本，然后重启应用（Windows 上安装程序会自行退出并重启应用）。 */
export async function installUpdate() {
  const u = updater.update;
  if (!u || updater.downloading) return;
  updater.downloading = true;
  updater.progress = null;
  updater.error = '';
  let total = 0;
  let received = 0;
  try {
    await u.downloadAndInstall((e) => {
      if (e.event === 'Started') {
        total = e.data.contentLength ?? 0;
        updater.progress = total ? 0 : null;
      } else if (e.event === 'Progress') {
        received += e.data.chunkLength;
        if (total) updater.progress = Math.min(100, Math.floor((received / total) * 100));
      } else {
        updater.progress = 100;
      }
    });
    await relaunch();
  } catch (e) {
    const msg = String(e);
    // 签名不对说明更新包不是官方发布的（或下载被篡改），给出明确的中文说明，原始错误附在后面
    updater.error = /signature/i.test(msg) ? `更新失败：更新包签名校验未通过，已取消安装（${msg}）` : `更新失败：${msg}`;
    updater.downloading = false;
  }
}
