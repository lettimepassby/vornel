// 官网的多语言页面与结构化数据。没有依赖，改了 public/index.html 或 i18n.en.mjs 之后运行：
//
//   node site/build.mjs          生成 public/en/index.html，并更新两页里的结构化数据（JSON-LD）
//   node site/build.mjs --check  只检查生成的文件是否最新（CI 用）
//
// 中文页 public/index.html 是手写的源文件；英文页由它按 i18n.en.mjs 翻译得到：
//   data-i18n="键"          整个元素内容换成译文（可以带 HTML）
//   data-i18n-<属性>="键"   该属性换成译文，例如 data-i18n-content、data-i18n-aria-label
// 英文页里不允许残留中文（语言切换按钮除外），漏翻会直接报错。
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import EN from './i18n.en.mjs';

const dir = path.dirname(fileURLToPath(import.meta.url));
const PUB = path.join(dir, 'public');
const SITE = 'https://vornel.dev';
const check = process.argv.includes('--check');

const LD_START = '<!-- seo:ld 由 site/build.mjs 生成 -->';
const LD_END = '<!-- /seo:ld -->';

const escAttr = (s) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
const unescape = (s) =>
  s
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&');
const text = (html) => unescape(html.replace(/<br\s*\/?>/gi, ' ').replace(/<[^>]+>/g, '')).replace(/\s+/g, ' ').trim();

/** 从 from 开始找 name 元素对应的结束标签（处理同名元素嵌套） */
function findClose(html, name, from) {
  const re = new RegExp(`<(/?)${name}\\b[^>]*>`, 'gi');
  re.lastIndex = from;
  let depth = 1;
  for (let m; (m = re.exec(html)); ) {
    if (m[1]) {
      if (--depth === 0) return m.index;
    } else if (!m[0].endsWith('/>')) depth++;
  }
  throw new Error(`<${name}> 没有结束标签`);
}

/** 读出所有 data-i18n 元素的内容：{ 键: innerHTML } */
function contents(html) {
  const out = {};
  const open = /<([a-zA-Z][\w-]*)\b[^>]*?\sdata-i18n="([^"]+)"[^>]*>/g;
  for (let m; (m = open.exec(html)); ) {
    const start = m.index + m[0].length;
    out[m[2]] = html.slice(start, findClose(html, m[1], start));
  }
  return out;
}

/** 按词典翻译：元素内容与属性 */
function translate(html, dict, used) {
  const get = (k) => {
    if (!(k in dict)) throw new Error(`i18n.en.mjs 缺少 '${k}'`);
    used.add(k);
    return dict[k];
  };
  // 先换属性（只改开始标签本身）
  html = html.replace(/<[a-zA-Z][^>]*\sdata-i18n-[a-z-]+="[^"]+"[^>]*>/g, (tag) => {
    for (const [, attr, key] of tag.matchAll(/\sdata-i18n-([a-z-]+)="([^"]+)"/g)) {
      const re = new RegExp(`(\\s${attr}=")[^"]*(")`);
      if (!re.test(tag)) throw new Error(`${tag} 没有 ${attr} 属性`);
      tag = tag.replace(re, (_, a, b) => a + escAttr(get(key)) + b);
    }
    return tag;
  });
  // 再换元素内容
  let out = '';
  let i = 0;
  const open = /<([a-zA-Z][\w-]*)\b[^>]*?\sdata-i18n="([^"]+)"[^>]*>/g;
  for (let m; (m = open.exec(html)); ) {
    const start = m.index + m[0].length;
    const end = findClose(html, m[1], start);
    out += html.slice(i, start) + get(m[2]);
    i = end;
    open.lastIndex = end;
  }
  return out + html.slice(i);
}

/** 结构化数据：软件、网站、常见问题 */
function jsonLd(lang, html) {
  const c = contents(html);
  const meta = (attr, v) => {
    const m = html.match(new RegExp(`<meta\\s[^>]*${attr}="${v}"[^>]*\\scontent="([^"]*)"`));
    if (!m) throw new Error(`找不到 <meta ${attr}="${v}">`);
    return unescape(m[1]);
  };
  const url = lang === 'en' ? `${SITE}/en/` : `${SITE}/`;
  const inLanguage = lang === 'en' ? 'en' : 'zh-CN';
  const faq = [];
  for (let n = 1; c[`faq.q${n}`]; n++) {
    faq.push({ '@type': 'Question', name: text(c[`faq.q${n}`]), acceptedAnswer: { '@type': 'Answer', text: text(c[`faq.a${n}`]) } });
  }
  const features = ['proto.ssh.t', 'proto.sftp.t', 'proto.rdp.t', 'proto.vnc.t', 'proto.mosh.t', 'proto.telnet.t', 'proto.serial.t', 'proto.fwd.t', 'split.toggle', 'sync.kicker'].map((k) => text(c[k]));
  const data = {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'SoftwareApplication',
        '@id': `${SITE}/#app`,
        name: 'Vornel',
        url,
        description: meta('name', 'description'),
        applicationCategory: 'DeveloperApplication',
        applicationSubCategory: lang === 'en' ? 'SSH client' : 'SSH 客户端',
        operatingSystem: 'macOS 13+, Windows 10, Windows 11',
        image: meta('property', 'og:image'),
        downloadUrl: `${url}#download`,
        installUrl: `${url}#download`,
        releaseNotes: 'https://github.com/lettimepassby/vornel/releases',
        license: 'https://polyformproject.org/licenses/noncommercial/1.0.0/',
        isAccessibleForFree: true,
        offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
        featureList: features,
        inLanguage: ['zh-CN', 'en'],
        sameAs: ['https://github.com/lettimepassby/vornel'],
      },
      { '@type': 'WebSite', '@id': `${SITE}/#site`, name: 'Vornel', url: `${SITE}/`, inLanguage },
      { '@type': 'FAQPage', '@id': `${url}#faq`, url, inLanguage, mainEntity: faq },
    ],
  };
  const json = JSON.stringify(data, null, 2).replace(/</g, '\\u003c');
  return `${LD_START}\n  <script type="application/ld+json">\n${json.replace(/^/gm, '  ')}\n  </script>\n  ${LD_END}`;
}

function setLd(html, block) {
  const a = html.indexOf(LD_START);
  const b = html.indexOf(LD_END);
  if (a < 0 || b < a) throw new Error('index.html 里找不到 seo:ld 标记');
  return html.slice(0, a) + block + html.slice(b + LD_END.length);
}

function replaceOnce(html, from, to) {
  if (!html.includes(from)) throw new Error(`找不到：${from}`);
  return html.replace(from, to);
}

// ------------------------------------------------------------ 中文页（源文件）：只更新结构化数据
const zhPath = path.join(PUB, 'index.html');
const zhSrc = fs.readFileSync(zhPath, 'utf8');
const zh = setLd(zhSrc, jsonLd('zh', zhSrc));

// ------------------------------------------------------------ 英文页
const used = new Set();
let en = translate(zh, EN, used);
en = replaceOnce(en, '<html lang="zh-CN"', '<html lang="en"');
en = replaceOnce(en, '<meta charset="utf-8" />', '<meta charset="utf-8" />\n  <!-- 由 site/build.mjs 根据 index.html 与 i18n.en.mjs 生成，不要直接修改 -->');
en = replaceOnce(en, '<link rel="canonical" href="https://vornel.dev/" />', '<link rel="canonical" href="https://vornel.dev/en/" />');
en = replaceOnce(en, '<meta property="og:url" content="https://vornel.dev/" />', '<meta property="og:url" content="https://vornel.dev/en/" />');
en = replaceOnce(en, '<meta property="og:locale" content="zh_CN" />\n  <meta property="og:locale:alternate" content="en_US" />', '<meta property="og:locale" content="en_US" />\n  <meta property="og:locale:alternate" content="zh_CN" />');
en = replaceOnce(
  en,
  '<a class="ghost-btn" id="lang" href="/en/" hreflang="en" lang="en" aria-label="English version">EN</a>',
  '<a class="ghost-btn" id="lang" href="/" hreflang="zh" lang="zh-CN" aria-label="中文版">中文</a>',
);
en = setLd(en, jsonLd('en', en));

// 漏翻检查：去掉注释与语言按钮后不应再有中文
const leftover = en
  .replace(/<!--[\s\S]*?-->/g, '')
  .replace(/<a class="ghost-btn" id="lang"[^>]*>中文<\/a>/, '')
  .match(/[^\n]*[㐀-鿿　-〿＀-￯][^\n]*/g);
if (leftover) throw new Error(`英文页还有中文：\n${leftover.map((l) => l.trim()).join('\n')}`);
const unused = Object.keys(EN).filter((k) => !used.has(k));
if (unused.length) throw new Error(`i18n.en.mjs 里这些键已经没有用到：${unused.join(', ')}`);

// ------------------------------------------------------------ 写出 / 检查
const outputs = [
  [zhPath, zh],
  [path.join(PUB, 'en', 'index.html'), en],
];
let stale = 0;
for (const [file, content] of outputs) {
  const cur = fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : null;
  if (cur === content) continue;
  const rel = path.relative(process.cwd(), file);
  if (check) {
    console.error(`${rel} 不是最新的，请运行 node site/build.mjs`);
    stale++;
  } else {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, content);
    console.log(`已生成 ${rel}`);
  }
}
if (stale) process.exit(1);
if (check) console.log('官网生成文件是最新的');
