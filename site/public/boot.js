// 在 <head> 里同步执行（不加 defer），赶在首次绘制之前：
// 1. 记住过语言的访客直接跳到对应的页面（中文 /，英文 /en/）。没选过的不自动跳——搜索引擎的爬虫
//    也是“没选过”的访客，这样两种语言的页面都能被原样收录；浏览器语言不同时由 app.js 提示切换。
// 2. 套用记住的主题，避免先闪一下深色。
(() => {
  let lang = null;
  let theme = null;
  try {
    lang = localStorage.getItem('vornel.lang');
    theme = localStorage.getItem('vornel.theme');
  } catch {
    /* 隐私模式等 */
  }
  const onEn = /^\/en(\/|$)/.test(location.pathname);
  if (lang === 'en' && !onEn) location.replace('/en/' + location.hash);
  else if (lang === 'zh' && onEn) location.replace('/' + location.hash);
  if (theme === 'paper') document.documentElement.dataset.theme = 'paper';
})();
