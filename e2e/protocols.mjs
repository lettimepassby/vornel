// 端到端测试（第二部分）：通过真实界面验证 Telnet、VNC、Mosh 与“本地文件夹”同步。
//
// 环境变量：
//   VORNEL_APP                              应用可执行文件路径
//   SSH_PORT / SSH_USER / SSH_PASS          真实 sshd（Mosh 先经 SSH 启动 mosh-server）
//   TELNET_PORT / TELNET_USER / TELNET_PASS 真实 telnetd（需要登录）
//   VNC_PORT / VNC_PASS                     真实 x11vnc（桌面底色 #3366cc）
// 截图写到 screenshots/p*.png，和 run.mjs 的截图放在一起。

import fs from 'node:fs';
import path from 'node:path';
import { Builder, By, Key, until } from 'selenium-webdriver';

const env = (k) => {
  const v = process.env[k];
  if (!v) throw new Error(`缺少环境变量 ${k}`);
  return v;
};

fs.mkdirSync('screenshots', { recursive: true });
const driver = await new Builder()
  .usingServer('http://127.0.0.1:4444/')
  .withCapabilities({ browserName: 'wry', 'tauri:options': { application: env('VORNEL_APP') } })
  .build();

let step = 0;
async function shot(name) {
  step += 1;
  const file = `screenshots/p${String(step).padStart(2, '0')}-${name}.png`;
  fs.writeFileSync(file, await driver.takeScreenshot(), 'base64');
  console.log(`  📸 ${file}`);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function find(locator, timeout = 20000) {
  const el = await driver.wait(until.elementLocated(locator), timeout);
  await driver.wait(until.elementIsVisible(el), timeout);
  return el;
}
// 沉浸模式下侧边栏是隐藏的：要点侧边栏里的东西时，先点标签栏的“主机”按钮把它调出来（用户的真实操作路径）
async function showSidebarIfNeeded(locator) {
  const q = String(locator.value ?? locator);
  if (!/aside|主导航|新建主机|"设置"/.test(q)) return;
  const home = await driver.findElements(By.css('button.home-btn'));
  if (home.length && (await home[0].isDisplayed())) await home[0].click();
}
async function click(locator) {
  await showSidebarIfNeeded(locator);
  const el = await find(locator);
  await el.click();
  return el;
}
/** 设置输入框的值并触发 input 事件（Svelte 的 bind:value 监听它）。 */
async function setValue(el, text) {
  await driver.executeScript(
    `const el = arguments[0];
     const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
     Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, arguments[1]);
     el.dispatchEvent(new Event('input', { bubbles: true }));`,
    el,
    text,
  );
}
/** 像用户一样键入；WebKitWebDriver 偶尔会打乱需要 Shift 的字符，打错了就直接改值。 */
async function type(locator, text) {
  const el = await find(locator);
  await el.sendKeys(Key.chord(Key.CONTROL, 'a'), Key.BACK_SPACE);
  await el.sendKeys(text);
  if ((await el.getAttribute('value')) !== text) await setValue(el, text);
  return el;
}
const field = (label) => By.xpath(`//label[span[starts-with(normalize-space(.), ${JSON.stringify(label)})]]//input`);
const button = (text) => By.xpath(`//button[contains(normalize-space(.), ${JSON.stringify(text)})]`);
const nav = (text) => By.xpath(`//nav[@aria-label="主导航"]//button[contains(., ${JSON.stringify(text)})]`);
const pane = (name) => `//div[@role="application"][@aria-label=${JSON.stringify(`终端 ${name}`)}]`;

// ---------------------------------------------------------------- 终端读取
async function terminalKeys() {
  return driver.executeScript('return Object.keys(window.__vornelTerminals || {});');
}
async function terminalOf(key) {
  return driver.executeScript('const f = (window.__vornelTerminals || {})[arguments[0]]; return f ? f() : "";', key);
}
/** 打开一个主机标签，返回它的终端键（主机 id）。 */
async function openTerminal(name) {
  const before = await terminalKeys();
  await click(By.xpath(`//aside//button[contains(., ${JSON.stringify(name)})]`));
  let key;
  await driver.wait(
    async () => {
      key = (await terminalKeys()).find((k) => !before.includes(k));
      return !!key;
    },
    20000,
    `${name} 的终端没有创建`,
  );
  return key;
}
async function waitText(key, pred, what, timeout = 30000) {
  let last = '';
  try {
    await driver.wait(
      async () => {
        last = await terminalOf(key);
        return typeof pred === 'string' ? last.includes(pred) : pred(last);
      },
      timeout,
    );
  } catch {
    throw new Error(`终端里没有等到 ${what}；终端末尾：${JSON.stringify(last.trimEnd().slice(-400))}`);
  }
  return last;
}
/** 点击终端获得焦点，再用真实键盘事件输入（只用不需要 Shift 的字符）。 */
async function keyboard(name, text) {
  await click(By.xpath(`${pane(name)}//div[contains(@class, "xterm-screen")]`));
  await driver.actions().sendKeys(text, Key.ENTER).perform();
}
const promptAfter = (marker) => (t) => {
  const i = t.lastIndexOf(marker);
  return i >= 0 && /\$\s*$/m.test(t.slice(i + marker.length));
};

async function addHost({ protocol, address, port, name, user }) {
  await click(nav('主机'));
  await click(By.css('button[aria-label="新建主机"]'));
  await find(By.xpath('//dialog//h2[contains(., "新建主机")]'));
  await click(By.xpath(`//div[@aria-label="协议"]/button[normalize-space(.)=${JSON.stringify(protocol)}]`));
  await type(field('地址'), address);
  await type(field('端口'), String(port));
  await type(field('名称'), name);
  if (user !== undefined) await type(field('用户名'), user);
  await shot(`dialog-${name}`);
  await click(By.xpath('//dialog//button[@type="submit"]'));
  await find(By.xpath(`//aside//button[contains(., ${JSON.stringify(name)})]`));
}

const results = [];
async function check(name, fn) {
  process.stdout.write(`▶ ${name}\n`);
  try {
    await fn();
    results.push([name, 'ok']);
    console.log(`  ✓ ${name}`);
  } catch (e) {
    results.push([name, String(e.message || e)]);
    console.log(`  ✗ ${name}：${e.message || e}`);
    await shot(`FAILED-${name}`).catch(() => {});
  }
}

try {
  await check('启动应用', async () => {
    await find(nav('主机'), 60000);
    await shot('start');
  });

  // ------------------------------------------------------------ Telnet
  await check('Telnet：登录真实 telnetd 并执行命令', async () => {
    // 密码含大写字母，用“代码片段”发送到终端（键盘事件里的 Shift 字符在 WebKitWebDriver 下不可靠）
    await click(nav('代码片段'));
    await click(button('新建片段'));
    await type(field('名称'), 'tn-pass');
    await type(By.css('dialog textarea'), env('TELNET_PASS'));
    await click(By.xpath('//dialog//button[@type="submit"]'));
    await find(By.xpath('//div[contains(@class,"item")][contains(., "tn-pass")]'));

    await addHost({ protocol: 'Telnet', address: '127.0.0.1', port: env('TELNET_PORT'), name: 'e2e-telnet', user: env('TELNET_USER') });
    const key = await openTerminal('e2e-telnet');
    await find(By.xpath('//div[@role="tablist"]//div[contains(@class,"on")]//span[contains(@class,"dot") and contains(@class,"ok")]'), 30000);
    await waitText(key, 'login:', '登录提示');
    await shot('telnet-login');
    await keyboard('e2e-telnet', env('TELNET_USER'));
    await waitText(key, 'assword:', '密码提示');
    await click(By.xpath('//div[contains(@class, "snip")]/button[contains(., "片段")]'));
    await click(By.xpath('//div[@role="menu"]/button[@role="menuitem"][contains(., "tn-pass")]'));
    await waitText(key, promptAfter('assword:'), '登录后的 shell 提示符');
    await keyboard('e2e-telnet', 'printf tn-; seq 42 42');
    await waitText(key, 'tn-42', 'tn-42');
    await shot('telnet-command');
  });

  // ------------------------------------------------------------ VNC
  await check('VNC：连接真实 x11vnc 并显示桌面', async () => {
    await addHost({ protocol: 'VNC', address: '127.0.0.1', port: env('VNC_PORT'), name: 'e2e-vnc' });
    await click(By.xpath('//aside//button[contains(., "e2e-vnc")]'));
    const canvas = '//canvas[@aria-label="远程桌面 e2e-vnc"]';
    const pw = await find(By.xpath(`${canvas}/following-sibling::div//input[@type="password"]`));
    await pw.sendKeys(env('VNC_PASS'));
    if ((await pw.getAttribute('value')) !== env('VNC_PASS')) await setValue(pw, env('VNC_PASS'));
    await pw.sendKeys(Key.ENTER);
    await find(By.xpath('//div[@role="tablist"]//div[contains(@class,"on")]//span[contains(@class,"dot") and contains(@class,"ok")]'), 45000);
    // 桌面底色是 #3366cc：既验证收到了画面，也验证了颜色通道顺序
    let stats;
    try {
      await driver.wait(async () => {
        stats = await driver.executeScript(`
          const c = document.querySelector('canvas[aria-label="远程桌面 e2e-vnc"]');
          if (!c || !c.width) return { w: 0 };
          const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
          let n = 0, blue = 0, lit = 0, sample = null;
          for (let i = 0; i < d.length; i += 4 * 101) {
            n++;
            if (d[i] + d[i + 1] + d[i + 2] > 30) { lit++; sample = sample || [d[i], d[i + 1], d[i + 2]]; }
            if (Math.abs(d[i] - 0x33) < 16 && Math.abs(d[i + 1] - 0x66) < 16 && Math.abs(d[i + 2] - 0xcc) < 16) blue++;
          }
          return { w: c.width, h: c.height, n, lit, blue, sample };`);
        return stats.w > 0 && stats.blue > stats.n * 0.5;
      }, 45000);
    } catch {
      throw new Error(`VNC 画面不是预期的 #3366cc 桌面：${JSON.stringify(stats)}`);
    }
    console.log(`  画面 ${stats.w}×${stats.h}，${stats.blue}/${stats.n} 个采样点为桌面底色`);
    // VNC 不用用户名，状态栏不应显示 SSH 默认的 root@
    // 沉浸模式隐藏了状态栏：先回到带侧边栏的布局再检查
    const home = await driver.findElements(By.css('button.home-btn'));
    if (home.length && (await home[0].isDisplayed())) await home[0].click();
    const footer = await (await find(By.css('footer'))).getText();
    if (footer.includes('root@')) throw new Error(`VNC 状态栏显示了无关的用户名：${footer}`);
    await sleep(1000);
    await shot('vnc');
  });

  // ------------------------------------------------------------ Mosh
  await check('Mosh：经 SSH 启动 mosh-server 后走 UDP 执行命令', async () => {
    await addHost({ protocol: 'Mosh', address: '127.0.0.1', port: env('SSH_PORT'), name: 'e2e-mosh', user: env('SSH_USER') });
    const key = await openTerminal('e2e-mosh');
    const pw = await find(By.xpath(`${pane('e2e-mosh')}//input[@type="password"]`));
    await pw.sendKeys(env('SSH_PASS'));
    if ((await pw.getAttribute('value')) !== env('SSH_PASS')) await setValue(pw, env('SSH_PASS'));
    await pw.sendKeys(Key.ENTER);
    await find(By.xpath('//div[@role="tablist"]//div[contains(@class,"on")]//span[contains(@class,"dot") and contains(@class,"ok")]'), 45000);
    // mosh 会整屏重绘（清掉之前的提示行），这个终端只属于 Mosh 会话，直接找行尾的提示符
    await waitText(key, (t) => /\$\s*$/m.test(t), 'shell 提示符', 45000);
    await shot('mosh-connected');
    await keyboard('e2e-mosh', 'printf moxx; seq 42 42');
    await waitText(key, 'moxx42', 'moxx42');
    await shot('mosh-command');
  });

  // ------------------------------------------------------------ 文件夹同步
  await check('同步：本地文件夹创建加密同步库', async () => {
    const dir = `/tmp/vornel-sync-${Date.now()}`;
    fs.mkdirSync(dir, { recursive: true });
    await click(By.css('button[aria-label="设置"]'));
    await click(By.xpath('//dialog//button[@role="tab"][normalize-space(.)="同步"]'));
    await click(By.xpath('//div[@aria-label="同步后端"]/button[normalize-space(.)="本地文件夹"]'));
    await type(By.xpath('//dialog//input[starts-with(@placeholder, "选择 iCloud Drive")]'), dir);
    await click(By.xpath('//dialog//button[normalize-space(.)="保存"]'));
    await find(By.xpath('//dialog//p[contains(@class, "ok")][contains(., "已保存")]'));
    await click(By.xpath('//dialog//button[contains(., "立即同步")]'));
    await find(By.xpath('//dialog//b[contains(., "设置主密码")]'), 30000);
    await type(By.xpath('//dialog//input[@placeholder="主密码"]'), 'sync-pass-1');
    await type(By.xpath('//dialog//input[@placeholder="再输入一次"]'), 'sync-pass-1');
    await shot('sync-password');
    await click(By.xpath('//dialog//button[contains(., "立即同步")]'));
    const ok = await find(By.xpath('//dialog//p[contains(@class, "ok")][contains(., "已创建云端同步库")]'), 60000);
    console.log(`  ${await ok.getText()}`);
    await shot('sync-done');

    const root = path.join(dir, 'vornel');
    for (const f of ['vault.json', 'manifest.bin']) {
      if (!fs.existsSync(path.join(root, f))) throw new Error(`同步目录里没有 vornel/${f}`);
    }
    const vault = JSON.parse(fs.readFileSync(path.join(root, 'vault.json'), 'utf8'));
    console.log(`  vault.json 字段：${Object.keys(vault).join(', ')}`);
    const records = fs.existsSync(path.join(root, 'records')) ? fs.readdirSync(path.join(root, 'records')) : [];
    if (!records.length) throw new Error('同步目录里没有任何加密记录（records/）');
    console.log(`  records/ 下有 ${records.length} 个文件`);
    // 云端只能看到密文：任何文件里都不应出现主机名、地址或用户名
    const files = ['vault.json', 'manifest.bin', ...records.map((r) => path.join('records', r))];
    for (const f of files) {
      const bytes = fs.readFileSync(path.join(root, f));
      for (const secret of ['e2e-telnet', 'e2e-mosh', 'e2e-vnc', '127.0.0.1', 'tn-pass', env('TELNET_PASS')]) {
        if (bytes.includes(Buffer.from(secret))) throw new Error(`vornel/${f} 中出现了明文 ${secret}`);
      }
    }
    await click(By.css('dialog button[aria-label="关闭"]'));
    // 关闭设置后侧栏应反映同步已开启（而不是一直显示“同步未开启”）
    await find(By.xpath('//aside//button[contains(@class, "sync-btn")][contains(., "已同步")]'), 30000);
    await shot('sync-sidebar');
  });
} finally {
  await driver.quit().catch(() => {});
  console.log('\n结果：');
  for (const [n, r] of results) console.log(`  ${r === 'ok' ? '✓' : '✗'} ${n}${r === 'ok' ? '' : `：${r}`}`);
  if (results.some(([, r]) => r !== 'ok')) process.exitCode = 1;
}
