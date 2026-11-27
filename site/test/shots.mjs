// 官网截图：用真实浏览器打开站点，截桌面（深色中文 / 浅色英文）与手机，检查横向溢出、脚本错误，
// 以及搜索引擎相关的标记（语言、hreflang、结构化数据、英文页不残留中文）。
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
  await page.goto(new URL(lang === 'en' ? '/en/' : '/', base).href, { waitUntil: 'networkidle' });
  const seo = await page.evaluate(() => ({
    path: location.pathname,
    lang: document.documentElement.lang,
    title: document.title,
    alternates: document.querySelectorAll('link[rel="alternate"][hreflang]').length,
    canonical: document.querySelector('link[rel="canonical"]')?.href,
    ld: [...document.querySelectorAll('script[type="application/ld+json"]')].map((s) => {
      try {
        return JSON.parse(s.textContent)['@graph'].map((n) => n['@type']).join(',');
      } catch (e) {
        return `JSON 错误：${e.message}`;
      }
    }),
    cjk: document.body.innerText.match(/[\u4e00-\u9fff]+/g) || [],
  }));
  const wantPath = lang === 'en' ? '/en/' : '/';
  if (seo.path !== wantPath) problems.push(`${name}: 停在了 ${seo.path}，应为 ${wantPath}`);
  if (seo.lang !== (lang === 'en' ? 'en' : 'zh-CN')) problems.push(`${name}: <html lang> 是 ${seo.lang}`);
  if (seo.alternates !== 3) problems.push(`${name}: hreflang 链接有 ${seo.alternates} 个，应为 3 个`);
  if (!seo.canonical?.endsWith(wantPath)) problems.push(`${name}: canonical 是 ${seo.canonical}`);
  if (seo.ld.join() !== 'SoftwareApplication,WebSite,FAQPage') problems.push(`${name}: 结构化数据不对：${seo.ld.join(' | ')}`);
  if (lang === 'en' && seo.cjk.join() !== '中文') problems.push(`${name}: 英文页里有中文：${seo.cjk.join(' ')}`);
  if (lang === 'zh' && !/SSH/.test(seo.title)) problems.push(`${name}: 标题里没有 SSH：${seo.title}`);
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

// 第一次来（没选过语言）、浏览器是英文：停在中文页、不自动跳转（搜索引擎的爬虫看到的也是这样），
// 底部提示有英文版；选过之后记住，再打开 / 直接去 /en/，英文页的语言按钮能回到中文
async function firstVisit() {
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, locale: 'en-US' });
  const page = await ctx.newPage();
  page.on('pageerror', (e) => problems.push(`first-visit: ${e.message}`));
  const at = () => new URL(page.url()).pathname;
  try {
    await page.goto(new URL('/', base).href, { waitUntil: 'networkidle' });
    if (at() !== '/') problems.push(`first-visit: 自动跳到了 ${at()}`);
    const tip = page.locator('.lang-tip');
    if (!(await tip.isVisible())) problems.push('first-visit: 英文浏览器打开中文页，没有提示英文版');
    await page.waitForTimeout(600);
    await page.screenshot({ path: 'shots/first-visit-tip.png' });
    await tip.getByRole('link').click();
    await page.waitForURL((u) => u.pathname === '/en/');
    await page.goto(new URL('/', base).href, { waitUntil: 'networkidle' });
    if (at() !== '/en/') problems.push(`first-visit: 选了英文之后打开 / 停在了 ${at()}`);
    await page.locator('#lang').click();
    await page.waitForURL((u) => u.pathname === '/');
    if (await page.locator('.lang-tip').count()) problems.push('first-visit: 选过语言后仍然提示');
  } catch (e) {
    problems.push(`first-visit: ${e.message}`);
  }
  await ctx.close();
}

await firstVisit();
await shot('desktop-ink-zh', { width: 1440, height: 900, theme: 'ink', lang: 'zh' });
await shot('desktop-paper-en', { width: 1440, height: 900, theme: 'paper', lang: 'en' });
await shot('mobile-ink-zh', { width: 390, height: 844, theme: 'ink', lang: 'zh', mobile: true });
await browser.close();
console.log(problems.length ? `问题：\n${problems.join('\n')}` : '没有发现脚本错误或横向溢出');
if (problems.length) process.exitCode = 1;
