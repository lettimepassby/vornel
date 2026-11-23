// 端到端测试：通过 tauri-driver 驱动真实编译出的 Vornel，像用户一样操作界面。
//
// 环境变量：
//   VORNEL_APP           应用可执行文件路径
//   SSH_PORT / SSH_USER / SSH_PASS   真实 sshd（密码认证）
//   RDP_PORT / RDP_USER / RDP_PASS   真实 xrdp
// 每一步都会截图到 screenshots/，供人工检查界面。

import fs from 'node:fs';
import net from 'node:net';
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
  const file = `screenshots/${String(step).padStart(2, '0')}-${name}.png`;
  fs.writeFileSync(file, await driver.takeScreenshot(), 'base64');
  console.log(`  📸 ${file}`);
}

const xText = (text, tag = '*') => By.xpath(`//${tag}[contains(normalize-space(.), ${JSON.stringify(text)})]`);
async function find(locator, timeout = 20000) {
  const el = await driver.wait(until.elementLocated(locator), timeout);
  await driver.wait(until.elementIsVisible(el), timeout);
  return el;
}
async function click(locator) {
  const el = await find(locator);
  await el.click();
  return el;
}
async function type(locator, text, clear = true) {
  const el = await find(locator);
  if (clear) {
    await el.sendKeys(Key.chord(Key.CONTROL, 'a'), Key.BACK_SPACE);
  }
  await el.sendKeys(text);
  return el;
}
const field = (label) => By.xpath(`//label[span[starts-with(normalize-space(.), ${JSON.stringify(label)})]]//input`);
const button = (text) => By.xpath(`//button[contains(normalize-space(.), ${JSON.stringify(text)})]`);

async function terminalText() {
  return driver.executeScript(
    'const t = window.__vornelTerminals || {}; return Object.values(t).map((f) => f()).join("\\n");',
  );
}
async function waitTerminal(needle, timeout = 30000) {
  await driver.wait(async () => (await terminalText()).includes(needle), timeout, `终端里没有出现 ${needle}`);
}

async function addHost({ protocol, address, port, name, user, password }) {
  await click(By.css('button[aria-label="新建主机"]'));
  await find(xText('新建主机', 'h2'));
  if (protocol) await click(By.xpath(`//div[@aria-label="协议"]/button[normalize-space(.)=${JSON.stringify(protocol)}]`));
  await type(field('地址'), address);
  await type(field('端口'), String(port));
  await type(field('名称'), name);
  if (user !== undefined) await type(field('用户名'), user);
  await shot(`dialog-${name}`);
  await click(By.xpath('//dialog//button[@type="submit"]'));
  await find(By.xpath(`//aside//button[contains(., ${JSON.stringify(name)})]`));
  void password;
}

function tcpBanner(port) {
  return new Promise((resolve, reject) => {
    const s = net.connect(port, '127.0.0.1');
    s.setTimeout(10000, () => reject(new Error('超时')));
    s.once('data', (d) => {
      resolve(d.toString());
      s.destroy();
    });
    s.once('error', reject);
  });
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
  await check('启动并显示欢迎页', async () => {
    await find(xText('欢迎使用 Vornel', 'h1'), 60000);
    await shot('welcome');
  });

  await check('新建 SSH 主机', async () => {
    await addHost({ address: '127.0.0.1', port: env('SSH_PORT'), name: 'e2e-ssh', user: env('SSH_USER') });
    await shot('host-added');
  });

  await check('输入密码连接真实 sshd', async () => {
    await click(By.xpath('//aside//button[contains(., "e2e-ssh")]'));
    await type(By.css('input[type="password"]'), env('SSH_PASS'));
    await click(button('连接'));
    await find(By.xpath('//footer[contains(., "已连接")]'), 30000);
    await waitTerminal('SHA256:');
    await shot('ssh-connected');
  });

  await check('终端执行命令并读回输出', async () => {
    const ta = await find(By.css('.xterm-helper-textarea'));
    await ta.sendKeys('echo e2e-$((6*7))', Key.ENTER);
    await waitTerminal('e2e-42');
    await shot('ssh-command');
  });

  await check('SFTP 双栏列出远端目录', async () => {
    await click(button('SFTP'));
    await find(By.xpath('//section[@aria-label="远端"]//button[@role="option"][contains(., ".profile") or contains(., ".bashrc")]'), 30000);
    await shot('sftp');
    await click(By.css('button[aria-label="关闭 SFTP"]'));
  });

  await check('生成 ed25519 密钥', async () => {
    await click(By.xpath('//nav[@aria-label="主导航"]//button[contains(., "密钥串")]'));
    await click(button('生成密钥'));
    await type(field('名称'), 'e2e-key');
    await click(By.xpath('//dialog//button[@type="submit"]'));
    await find(By.xpath('//div[contains(@class,"item")][contains(., "e2e-key")][contains(., "ssh-ed25519")]'));
    await shot('keys');
  });

  await check('代码片段发送到终端', async () => {
    await click(By.xpath('//nav[@aria-label="主导航"]//button[contains(., "代码片段")]'));
    await click(button('新建片段'));
    await type(field('名称'), 'answer');
    await type(By.css('dialog textarea'), 'echo snip-$((7*6))');
    await click(By.xpath('//dialog//button[@type="submit"]'));
    await click(By.xpath('//div[contains(@class,"item")][contains(., "answer")]//button[contains(., "发送")]'));
    await waitTerminal('snip-42');
    await shot('snippet');
  });

  await check('已知主机记录了 sshd 指纹', async () => {
    await click(By.xpath('//nav[@aria-label="主导航"]//button[contains(., "已知主机")]'));
    await find(By.xpath(`//div[contains(@class,"item")][contains(., "127.0.0.1:${env('SSH_PORT')}")][contains(., "SHA256:")]`));
    await shot('known-hosts');
  });

  await check('本地端口转发真实可用', async () => {
    await click(By.xpath('//nav[@aria-label="主导航"]//button[contains(., "端口转发")]'));
    await click(button('新建规则'));
    await type(field('名称'), 'e2e-fwd');
    await type(field('监听端口'), '18080');
    await type(By.xpath('//label[span[contains(., "目标地址")]]//input'), '127.0.0.1');
    await type(field('目标端口'), env('SSH_PORT'));
    await click(By.xpath('//dialog//button[@type="submit"]'));
    await click(By.xpath('//div[contains(@class,"item")][contains(., "e2e-fwd")]//button[contains(., "启动")]'));
    await type(By.css('dialog input[type="password"]'), env('SSH_PASS'));
    await click(By.xpath('//dialog//button[@type="submit"]'));
    await find(By.xpath('//div[contains(@class,"item")][contains(., "e2e-fwd")][contains(., "运行中")]'), 30000);
    const banner = await tcpBanner(18080);
    if (!banner.startsWith('SSH-2.0')) throw new Error(`经转发读到：${banner}`);
    await shot('forward');
  });

  await check('切换浅色主题', async () => {
    await click(By.css('button[aria-label="设置"]'));
    await click(button('浅色 Paper'));
    await driver.wait(async () => (await driver.executeScript('return document.documentElement.dataset.theme')) === 'paper', 5000);
    await shot('settings-paper');
    await click(By.css('dialog button[aria-label="关闭"]'));
  });

  await check('RDP 连接真实 xrdp 并显示画面', async () => {
    await click(By.xpath('//nav[@aria-label="主导航"]//button[contains(., "主机")]'));
    await addHost({ protocol: 'RDP', address: '127.0.0.1', port: env('RDP_PORT'), name: 'e2e-rdp', user: env('RDP_USER') });
    await click(By.xpath('//aside//button[contains(., "e2e-rdp")]'));
    await type(By.css('input[type="password"]'), env('RDP_PASS'));
    await click(button('连接'));
    await find(By.xpath('//footer[contains(., "已连接")]'), 45000);
    // 画布上应当有非黑色像素（收到了真实画面）
    await driver.wait(
      async () =>
        driver.executeScript(`
          const c = document.querySelector('canvas[aria-label^="远程桌面"]');
          if (!c || !c.width) return false;
          const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
          for (let i = 0; i < d.length; i += 4 * 97) if (d[i] + d[i + 1] + d[i + 2] > 30) return true;
          return false;`),
      45000,
      'RDP 画布一直是黑的',
    );
    await new Promise((r) => setTimeout(r, 2000));
    await shot('rdp');
  });
} finally {
  await driver.quit().catch(() => {});
  console.log('\n结果：');
  for (const [n, r] of results) console.log(`  ${r === 'ok' ? '✓' : '✗'} ${n}${r === 'ok' ? '' : `：${r}`}`);
  if (results.some(([, r]) => r !== 'ok')) process.exitCode = 1;
}
