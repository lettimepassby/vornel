// Vornel 官网的 Worker：/api/release 读取 GitHub 最新发布，给下载按钮提供各平台安装包地址。
// 结果在边缘缓存 10 分钟，GitHub 不可用时返回发布页链接作为兜底。

const REPO = 'lettimepassby/vornel';
const RELEASES = `https://github.com/${REPO}/releases`;

/** 从发布附件里挑出各平台的安装包 */
function pick(assets) {
  const find = (re) => assets.find((a) => re.test(a.name));
  const view = (a) => (a ? { name: a.name, url: a.browser_download_url, size: a.size } : null);
  return {
    macArm: view(find(/_aarch64\.dmg$/)),
    macIntel: view(find(/_x64\.dmg$/)),
    winExe: view(find(/-setup\.exe$/)),
    winMsi: view(find(/\.msi$/)),
  };
}

/** 安装包大小：HEAD 请求（跟随跳转），失败时为 0 */
async function sizeOf(url) {
  try {
    const r = await fetch(url, { method: 'HEAD', redirect: 'follow', headers: { 'User-Agent': 'vornel-site' } });
    return r.ok ? Number(r.headers.get('content-length') || 0) : 0;
  } catch {
    return 0;
  }
}

/**
 * 最新版本：读取发布附件里的更新清单 latest.json（普通文件下载，不受 GitHub API 按 IP 的限流影响——
 * Cloudflare 的出口 IP 由很多人共用，直接调 API 经常被限流），安装包地址按固定命名规则拼出。
 */
async function fromManifest() {
  const r = await fetch(`${RELEASES}/latest/download/latest.json`, { headers: { 'User-Agent': 'vornel-site' } });
  if (!r.ok) throw new Error(`latest.json ${r.status}`);
  const m = await r.json();
  const v = String(m.version || '').replace(/^v/, '');
  if (!v) throw new Error('latest.json 没有版本号');
  const base = `${RELEASES}/download/v${v}/`;
  const files = { macArm: `Vornel_${v}_aarch64.dmg`, macIntel: `Vornel_${v}_x64.dmg`, winExe: `Vornel_${v}_x64-setup.exe`, winMsi: `Vornel_${v}_x64_en-US.msi` };
  const entries = await Promise.all(
    Object.entries(files).map(async ([k, name]) => {
      const size = await sizeOf(base + name);
      return [k, size ? { name, url: base + name, size } : null];
    }),
  );
  return { ok: true, version: v, publishedAt: m.pub_date, page: `${RELEASES}/tag/v${v}`, assets: Object.fromEntries(entries) };
}

/** 备用：GitHub API（可能被限流） */
async function fromApi() {
  const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
    headers: { 'User-Agent': 'vornel-site', Accept: 'application/vnd.github+json' },
  });
  if (!r.ok) throw new Error(`GitHub API ${r.status}`);
  const rel = await r.json();
  return {
    ok: true,
    version: String(rel.tag_name || '').replace(/^v/, ''),
    publishedAt: rel.published_at,
    page: rel.html_url || RELEASES,
    assets: pick(rel.assets || []),
  };
}

async function latestRelease(ctx) {
  const cache = caches.default;
  const key = new Request('https://vornel.cache/api/release/v2');
  const hit = await cache.match(key);
  if (hit) return hit;

  let body;
  try {
    body = await fromManifest();
  } catch (e1) {
    try {
      body = await fromApi();
    } catch (e2) {
      body = { ok: false, page: `${RELEASES}/latest`, error: `${e1.message}; ${e2.message}` };
    }
  }

  const res = new Response(JSON.stringify(body), {
    headers: {
      'content-type': 'application/json; charset=utf-8',
      'cache-control': `public, max-age=${body.ok ? 600 : 60}`,
      'access-control-allow-origin': '*',
    },
  });
  if (body.ok) ctx.waitUntil(cache.put(key, res.clone()));
  return res;
}

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    if (url.pathname === '/api/release') return latestRelease(ctx);
    // 下载直达：/download/mac-arm、/download/windows 等，便于分享
    if (url.pathname.startsWith('/api/download/')) {
      const res = await latestRelease(ctx);
      const data = await res.clone().json();
      const which = url.pathname.split('/').pop();
      const map = { 'mac-arm': 'macArm', 'mac-intel': 'macIntel', windows: 'winExe', msi: 'winMsi' };
      const a = data.ok && data.assets[map[which]];
      return Response.redirect(a ? a.url : data.page, 302);
    }
    return env.ASSETS.fetch(request);
  },
};
