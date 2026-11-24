// 设计规范一致性测试：直接读取 DESIGN.md 里的表格，和真实运行的 Vornel 逐项对照。
// 规范改了而实现没跟上（或反过来），这里就会失败。
//
// 环境变量：VORNEL_APP 应用可执行文件路径

import fs from 'node:fs';
import { Builder, By, Key, until } from 'selenium-webdriver';

const DESIGN = fs.readFileSync(new URL('../DESIGN.md', import.meta.url), 'utf8');
const ICON = fs.readFileSync(new URL('../apps/vornel/app-icon.svg', import.meta.url), 'utf8');

fs.mkdirSync('screenshots', { recursive: true });
const driver = await new Builder()
  .usingServer('http://127.0.0.1:4444/')
  .withCapabilities({ browserName: 'wry', 'tauri:options': { application: process.env.VORNEL_APP } })
  .build();

async function shot(name) {
  fs.writeFileSync(`screenshots/design-${name}.png`, await driver.takeScreenshot(), 'base64');
}
async function find(locator, timeout = 20000) {
  const el = await driver.wait(until.elementLocated(locator), timeout);
  await driver.wait(until.elementIsVisible(el), timeout);
  return el;
}
const js = (code, ...args) => driver.executeScript(code, ...args);

/** 取 DESIGN.md 某一节下的第一张表，返回各行单元格 */
function table(heading) {
  const start = DESIGN.indexOf(heading);
  if (start < 0) throw new Error(`DESIGN.md 里没有 ${heading}`);
  const rows = [];
  for (const line of DESIGN.slice(start).split('\n').slice(1)) {
    if (line.startsWith('#')) break;
    if (!line.startsWith('|')) {
      if (rows.length) break;
      continue;
    }
    const cells = line.split('|').slice(1, -1).map((c) => c.trim());
    if (cells.every((c) => /^-+$/.test(c))) continue;
    rows.push(cells);
  }
  return rows.slice(1); // 去掉表头
}
const code = (s) => (s.match(/`([^`]+)`/) || [, s])[1];
const norm = (s) => s.replace(/\s+/g, '').toLowerCase();

function hexRgb(h) {
  const m = h.replace('#', '');
  return [0, 2, 4].map((i) => parseInt(m.slice(i, i + 2), 16) / 255);
}
function luminance(h) {
  const [r, g, b] = hexRgb(h).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function contrast(a, b) {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
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

const TOKENS = table('### 3.1 界面令牌').map(([t, , ink, paper]) => ({ token: code(t), ink: code(ink), paper: code(paper) }));
const PACKS = table('### 4.2 字体包').map(([name, ui, mono, term]) => ({ name: name.replace(/\*|（默认）/g, '').trim(), ui, mono, term }));

try {
  await find(By.css('nav[aria-label="主导航"]'), 60000);

  await check('Logo：几何与 DESIGN.md 2.1 一致', async () => {
    if (!ICON.includes('d="M12 22 L24 45 L36 22"')) throw new Error('应用图标里 v 的折线不一致');
    if (!/<rect x="42" y="17" width="10" height="30" rx="2.5"/.test(ICON)) throw new Error('应用图标里光标矩形不一致');
    const inApp = await js(`const s = document.querySelector('svg path[d="M12 22 L24 45 L36 22"]'); return !!s;`);
    if (!inApp) throw new Error('界面里的 Logo 不是规范的 v 折线');
  });

  for (const theme of ['ink', 'paper']) {
    await check(`颜色令牌（${theme}）与 DESIGN.md 3.1 一致`, async () => {
      if (TOKENS.length < 10) throw new Error(`只解析到 ${TOKENS.length} 个令牌`);
      const actual = await js(
        `document.documentElement.dataset.theme = arguments[0];
         const cs = getComputedStyle(document.documentElement);
         return Object.fromEntries(arguments[1].map((t) => [t, cs.getPropertyValue(t).trim()]));`,
        theme,
        TOKENS.map((t) => t.token),
      );
      const bad = TOKENS.filter((t) => norm(actual[t.token]) !== norm(t[theme])).map((t) => `${t.token} 规范 ${t[theme]} 实际 ${actual[t.token] || '(未定义)'}`);
      if (bad.length) throw new Error(bad.join('；'));
    });

    await check(`对比度（${theme}）满足 DESIGN.md 3.3`, async () => {
      const v = Object.fromEntries(TOKENS.map((t) => [t.token, t[theme]]));
      const bad = [];
      for (const fg of ['--fg', '--mut']) {
        for (const bg of ['--win', '--sb', '--raise', '--pane']) {
          const c = contrast(v[fg], v[bg]);
          if (c < 4.5) bad.push(`${fg} 在 ${bg} 上只有 ${c.toFixed(2)}:1`);
        }
      }
      for (const st of ['--acc', '--ok', '--warn', '--err']) {
        const c = contrast(v[st], v['--win']);
        if (c < 3) bad.push(`${st} 在 --win 上只有 ${c.toFixed(2)}:1`);
      }
      if (bad.length) throw new Error(bad.join('；'));
    });
  }
  await js(`document.documentElement.dataset.theme = 'ink';`);

  await check('尺寸：侧边栏顶栏 46、条目 29、状态栏 27（DESIGN.md 5.3 / 6）', async () => {
    const m = await js(`
      const h = (q) => { const e = document.querySelector(q); return e ? Math.round(e.getBoundingClientRect().height) : null; };
      return { top: h('aside .top'), item: h('nav[aria-label="主导航"] button'), status: h('footer.status') };`);
    const bad = [];
    if (m.top !== 46) bad.push(`侧边栏顶栏 ${m.top}`);
    if (m.item !== 29) bad.push(`侧边栏条目 ${m.item}`);
    if (m.status !== 27) bad.push(`状态栏 ${m.status}`);
    if (bad.length) throw new Error(bad.join('；'));
  });

  await check('可访问性：纯图标按钮都有 aria-label（DESIGN.md 8）', async () => {
    const scan = `return [...document.querySelectorAll('button')].filter((b) => b.offsetParent !== null && !b.textContent.trim() && !b.getAttribute('aria-label') && !b.getAttribute('title')).map((b) => b.outerHTML.slice(0, 120));`;
    const main = await js(scan);
    await (await find(By.css('button[aria-label="新建主机"]'))).click();
    await find(By.xpath('//dialog//h2[contains(., "新建主机")]'));
    const dlg = await js(scan);
    const bad = [...main, ...dlg];
    if (bad.length) throw new Error(bad.join('\n'));
  });

  await check('对话框：默认宽 440，Esc 关闭（DESIGN.md 6）', async () => {
    const w = await js(`return Math.round(document.querySelector('dialog[open]').getBoundingClientRect().width);`);
    if (w !== 440) throw new Error(`新建主机对话框宽 ${w}`);
    await driver.actions().sendKeys(Key.ESCAPE).perform();
    await driver.wait(async () => !(await js(`return !!document.querySelector('dialog[open]');`)), 5000, 'Esc 没有关闭对话框');
  });

  await check('输入框高 32（DESIGN.md 6）', async () => {
    const h = await js(`return Math.round(document.querySelector('aside input').getBoundingClientRect().height);`);
    if (h !== 32) throw new Error(`搜索框高 ${h}`);
  });

  await check('字体包：6 套都可切换且字体真实加载（DESIGN.md 4.2）', async () => {
    if (PACKS.length !== 6) throw new Error(`DESIGN.md 里解析到 ${PACKS.length} 套字体包`);
    await (await find(By.css('button[aria-label="设置"]'))).click();
    const bad = [];
    for (const p of PACKS) {
      const btn = await find(By.xpath(`//dialog//button[contains(@class,"pk")][contains(., ${JSON.stringify(p.name)})]`));
      await btn.click();
      const ok = await driver
        .wait(
          () =>
            js(
              `const cs = getComputedStyle(document.documentElement);
               const ui = cs.getPropertyValue('--f-ui'), mono = cs.getPropertyValue('--f-mono');
               return ui.includes(arguments[0]) && mono.includes(arguments[1]) &&
                 document.fonts.check('13px "' + arguments[0] + '"') && document.fonts.check('13px "' + arguments[1] + '"');`,
              p.ui,
              p.mono,
            ),
          8000,
        )
        .catch(() => false);
      if (!ok) bad.push(`${p.name}（${p.ui} / ${p.mono}）`);
    }
    await shot('font-packs');
    await (await find(By.xpath(`//dialog//button[contains(@class,"pk")][contains(., "现代")]`))).click();
    if (bad.length) throw new Error(`没有正确加载：${bad.join('、')}`);
  });

  await check('屏幕阅读器模式开关、选中即复制开关存在（DESIGN.md 8）', async () => {
    await find(By.xpath('//dialog//label[contains(., "屏幕阅读器模式")]//input[@type="checkbox"]'));
    await find(By.xpath('//dialog//label[contains(., "选中即复制")]//input[@type="checkbox"]'));
    await driver.actions().sendKeys(Key.ESCAPE).perform();
  });

  await check('快捷键调整终端字号（DESIGN.md 8：桌面 ⌘+ / ⌘−）', async () => {
    const size = () => js(`return JSON.parse(localStorage.getItem('vornel.prefs') || '{}').terminalFontSize;`);
    const before = await size();
    await driver.actions().keyDown(Key.CONTROL).sendKeys('=').keyUp(Key.CONTROL).perform();
    await driver.wait(async () => (await size()) === before + 1, 5000, `Ctrl+= 后字号没有变大（${before}）`);
    await driver.actions().keyDown(Key.CONTROL).sendKeys('0').keyUp(Key.CONTROL).perform();
    await driver.wait(async () => (await size()) === 13, 5000, 'Ctrl+0 没有恢复默认字号');
  });
} finally {
  await driver.quit().catch(() => {});
  console.log('\n结果：');
  for (const [n, r] of results) console.log(`  ${r === 'ok' ? '✓' : '✗'} ${n}${r === 'ok' ? '' : `：${r}`}`);
  if (results.some(([, r]) => r !== 'ok')) process.exitCode = 1;
}
