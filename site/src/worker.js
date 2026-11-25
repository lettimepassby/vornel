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

async function latestRelease(ctx) {
  const cache = caches.default;
  const key = new Request('https://vornel.cache/api/release');
  const hit = await cache.match(key);
  if (hit) return hit;

  let body;
  try {
    const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { 'User-Agent': 'vornel-site', Accept: 'application/vnd.github+json' },
    });
    if (!r.ok) throw new Error(`GitHub ${r.status}`);
    const rel = await r.json();
    body = {
      ok: true,
      version: String(rel.tag_name || '').replace(/^v/, ''),
      publishedAt: rel.published_at,
      page: rel.html_url || RELEASES,
      assets: pick(rel.assets || []),
    };
  } catch (e) {
    body = { ok: false, page: `${RELEASES}/latest`, error: String(e.message || e) };
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
