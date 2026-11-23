// 应用内自动更新端到端测试：用 tauri-driver 驱动一个旧版本的 Vornel AppImage，
// 从本机的 latest.json 检查更新、点“更新并重启”，再核对磁盘上的 AppImage。
//
// 环境变量：
//   VORNEL_APP      旧版本 AppImage 的路径（可写，更新会直接替换这个文件）
//   MODE            ok：签名正确，应当装上新版本；bad：签名不对，应当报错且文件不变；
//                   verify：启动已更新的 AppImage，确认它就是新版本、且不再提示更新
//   NEW_VERSION     latest.json 里的版本号
//   RUN_VERSION     启动的这个 AppImage 自身的版本号（状态栏与“关于”里应当显示它）
//   OLD_SHA         启动前 AppImage 的 sha256
//   NEW_SHA         新版本 AppImage 的 sha256（MODE=ok 时必填）
//   SHOT_TAG        截图文件名前缀（默认同 MODE）
// 截图保存到 screenshots/updater-<SHOT_TAG>-*.png。

import crypto from 'node:crypto';
import fs from 'node:fs';
import { Builder, By, until } from 'selenium-webdriver';

const env = (k) => {
  const v = process.env[k];
  if (!v) throw new Error(`缺少环境变量 ${k}`);
  return v;
};
const app = env('VORNEL_APP');
const mode = env('MODE');
const newVersion = env('NEW_VERSION');
const runVersion = env('RUN_VERSION');
const oldSha = env('OLD_SHA');
const newSha = mode === 'ok' ? env('NEW_SHA') : '';
const tag = process.env.SHOT_TAG || mode;

const sha256 = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const withTimeout = (p, ms) => Promise.race([p, sleep(ms).then(() => Promise.reject(new Error(`超时 ${ms}ms`)))]);

if (sha256(app) !== oldSha) throw new Error('启动前 AppImage 的 sha256 与 OLD_SHA 不一致');

fs.mkdirSync('screenshots', { recursive: true });
const driver = await new Builder()
  .usingServer('http://127.0.0.1:4444/')
  .withCapabilities({ browserName: 'wry', 'tauri:options': { application: app } })
  .build();

let step = 0;
async function shot(name) {
  step += 1;
  const file = `screenshots/updater-${tag}-${String(step).padStart(2, '0')}-${name}.png`;
  fs.writeFileSync(file, await withTimeout(driver.takeScreenshot(), 15000), 'base64');
  console.log(`  📸 ${file}`);
}

const notice = By.xpath(`//div[contains(@class,"notice")][contains(normalize-space(.), "Vornel") or contains(normalize-space(.), "新版本") or contains(normalize-space(.), "更新失败")]`);
const noticeText = async () => {
  const els = await driver.findElements(notice);
  return els.length ? (await els[0].getText()).trim() : '';
};

const footerVersion = By.xpath(`//footer[contains(normalize-space(.), ${JSON.stringify(`Vornel ${runVersion}`)})]`);

async function verifyUpdated() {
  // 状态栏显示的是新版本
  await driver.wait(until.elementLocated(footerVersion), 90000, `状态栏没有显示 Vornel ${runVersion}`);
  console.log(`状态栏：Vornel ${runVersion}`);
  // 设置 → 关于：版本号正确，手动检查更新得到“已是最新版本”
  await driver.findElement(By.css('button[aria-label="设置"]')).click();
  await driver.wait(until.elementLocated(By.xpath('//button[@role="tab"][contains(., "关于与开源许可")]')), 20000).then((b) => b.click());
  await driver.wait(until.elementLocated(By.xpath(`//b[normalize-space(.)=${JSON.stringify(`Vornel ${runVersion}`)}]`)), 20000);
  await driver.findElement(By.xpath('//button[contains(normalize-space(.), "检查更新")]')).click();
  const ok = await driver.wait(until.elementLocated(By.xpath('//p[contains(normalize-space(.), "已是最新版本")]')), 60000, '没有显示“已是最新版本”');
  console.log(`关于：Vornel ${runVersion}，${(await ok.getText()).trim()}`);
  await sleep(1000);
  await shot('about');
  if ((await driver.findElements(By.xpath('//div[contains(@class,"notice")][contains(., "发现新版本")]'))).length) {
    throw new Error('已是新版本，却仍然提示发现新版本');
  }
  if (sha256(app) !== oldSha) throw new Error('AppImage 被意外改动');
  console.log('✓ 更新后的 AppImage 以新版本运行，且不再提示更新');
}

let failed = '';
try {
  if (mode === 'verify') {
    await verifyUpdated();
  } else {
    // 状态栏显示旧版本
    await driver.wait(until.elementLocated(footerVersion), 90000, `状态栏没有显示 Vornel ${runVersion}`);
    console.log(`状态栏：Vornel ${runVersion}`);

    // 1. 启动后的静默检查应当发现新版本，显示提示条
    const banner = By.xpath(`//div[contains(@class,"notice")][.//span[normalize-space(.)=${JSON.stringify(`发现新版本 ${newVersion}`)}]]`);
    const el = await driver.wait(until.elementLocated(banner), 90000, `没有出现“发现新版本 ${newVersion}”提示条`);
    await driver.wait(until.elementIsVisible(el), 10000);
    console.log(`提示条：${(await el.getText()).trim()}`);
    await sleep(1500); // 让 WebView 画完再截图，否则可能是空白
    await shot('banner');

    // 2. 点“更新并重启”
    await el.findElement(By.xpath('.//button[contains(normalize-space(.), "更新并重启")]')).click();
    console.log('已点击“更新并重启”');

    if (mode === 'ok') {
      // 3. 等磁盘上的 AppImage 被替换；应用随后会重启，WebDriver 会话可能随之断开
      const deadline = Date.now() + 240000;
      let shotDownloading = false;
      let sessionAlive = true;
      let sha = oldSha;
      while (Date.now() < deadline) {
        if (sessionAlive) {
          try {
            const t = await withTimeout(noticeText(), 10000);
            if (t) console.log(`  提示条：${t}`);
            if (t.includes('更新失败')) {
              await shot('error').catch(() => {});
              throw new Error(`更新失败：${t}`);
            }
            if (!shotDownloading && t.includes('正在下载')) {
              shotDownloading = true;
              await shot('downloading').catch(() => {});
            }
          } catch (e) {
            if (String(e.message || e).startsWith('更新失败')) throw e;
            console.log(`  WebDriver 会话已断开（应用正在重启）：${String(e.message || e).split('\n')[0]}`);
            sessionAlive = false;
          }
        }
        try {
          sha = sha256(app);
        } catch {
          // 替换过程中文件可能短暂不存在
        }
        if (sha !== oldSha) {
          // 写入可能尚未结束，稍等后再读一次
          await sleep(3000);
          sha = sha256(app);
          break;
        }
        await sleep(1000);
      }
      console.log(`sha256 更新前：${oldSha}`);
      console.log(`sha256 更新后：${sha}`);
      console.log(`sha256 新版本：${newSha}`);
      if (sha !== newSha) throw new Error('磁盘上的 AppImage 没有变成新版本');
      const st = fs.statSync(app);
      if (!(st.mode & 0o111)) throw new Error('更新后的 AppImage 失去了可执行权限');
      console.log('✓ 已安装新版本，AppImage 已替换且保留可执行权限');
    } else {
      // 3. 签名不对：必须在界面上看到更新失败，且文件不变
      const err = By.xpath('//div[contains(@class,"notice")]//span[starts-with(normalize-space(.), "更新失败")]');
      const e = await driver.wait(until.elementLocated(err), 180000, '没有出现“更新失败”提示');
      const t = (await e.getText()).trim();
      console.log(`错误提示：${t}`);
      await sleep(1000);
      await shot('error');
      if (!t.includes('签名校验未通过')) throw new Error(`错误提示里没有说明签名校验未通过：${t}`);
      // 提示条恢复为可重试状态（按钮还在）
      await driver.findElement(By.xpath('//div[contains(@class,"notice")]//button[contains(normalize-space(.), "更新并重启")]'));
      await sleep(5000);
      const sha = sha256(app);
      console.log(`sha256 之前：${oldSha}`);
      console.log(`sha256 之后：${sha}`);
      if (sha !== oldSha) throw new Error('签名无效时 AppImage 仍被改动了');
      console.log('✓ 签名无效的更新被拒绝，AppImage 未改动');
    }
  }
} catch (e) {
  failed = String(e.message || e);
  console.log(`✗ ${failed}`);
  await shot('FAILED').catch(() => {});
} finally {
  await withTimeout(driver.quit(), 15000).catch(() => {});
}

if (failed) process.exit(1);
