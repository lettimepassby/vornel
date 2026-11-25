// 官网截图：用真实浏览器打开站点，截桌面（深色中文 / 浅色英文）与手机，检查横向溢出与脚本错误。
import { chromium } from 'playwright';
import fs from 'node:fs';

const base = process.env.SITE_URL || 'http://127.0.0.1:8788';
fs.mkdirSync('shots', { recursive: true });
const browser = await chromium.launch();
const problems = [];

async function shot(name, { width, height, theme, lang, mobile = false }) {
  const ctx = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: 1, isMobile: mobile, hasTouch: mobile, locale: lang === 'en' ? 'en-US' : 'zh-CN' });
  await ctx.addInitScript(([t, l]) => {
    localStorage.setItem('vornel.theme', t);
    localStorage.setItem('vornel.lang', l);
  }, [theme, lang]);
  const page = await ctx.newPage();
  page.on('pageerror', (e) => problems.push(`${name}: ${e.message}`));
  // 本地静态服务没有 Worker，/api/release 404 属于预期
  page.on('console', (m) => m.type() === 'error' && !(base.startsWith('http://127.') && /404/.test(m.text())) && !/cloudflareinsights/.test(m.text()) && problems.push(`${name}: console ${m.text()}`));
  await page.goto(base, { waitUntil: 'networkidle' });
  // 滚动一遍，触发所有出现动画
  const h = await page.evaluate(() => document.body.scrollHeight);
  for (let y = 0; y < h; y += height / 2) {
    await page.evaluate((v) => scrollTo(0, v), y);
    await page.waitForTimeout(120);
  }
  await page.evaluate(() => scrollTo(0, 0));
  await page.waitForTimeout(3500);
  // 下载区必须拿到真实版本号
  const ver = await page.evaluate(() => document.querySelector('#dl-version')?.textContent);
  if (!base.startsWith('http://127.') && !/^v\d+\.\d+\.\d+$/.test(ver || '')) problems.push(`${name}: 没有显示最新版本（${ver}）`);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
  if (overflow > 1) problems.push(`${name}: 页面横向溢出 ${overflow}px`);
  await page.screenshot({ path: `shots/${name}-top.png` });
  await page.screenshot({ path: `shots/${name}-full.png`, fullPage: true });
  await ctx.close();
}

await shot('desktop-ink-zh', { width: 1440, height: 900, theme: 'ink', lang: 'zh' });
await shot('desktop-paper-en', { width: 1440, height: 900, theme: 'paper', lang: 'en' });
await shot('mobile-ink-zh', { width: 390, height: 844, theme: 'ink', lang: 'zh', mobile: true });
await browser.close();
console.log(problems.length ? `问题：\n${problems.join('\n')}` : '没有发现脚本错误或横向溢出');
if (problems.length) process.exitCode = 1;
