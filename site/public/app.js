// Vornel 官网交互：语言 / 主题、终端演示、配色切换、资源栏、分屏同步输入、主机卡片、下载。
(() => {
  'use strict';
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const store = {
    get: (k) => {
      try {
        return localStorage.getItem(k);
      } catch {
        return null;
      }
    },
    set: (k, v) => {
      try {
        localStorage.setItem(k, v);
      } catch {
        /* 隐私模式等 */
      }
    },
  };

  // ------------------------------------------------------------ 语言
  const zh = {};
  $$('[data-i18n]').forEach((el) => (zh[el.dataset.i18n] = el.innerHTML));
  let lang = store.get('vornel.lang') || (/^zh/i.test(navigator.language) ? 'zh' : 'en');
  const t = (k) => (lang === 'en' ? window.I18N_EN[k] : window.I18N_ZH_DYN[k] ?? zh[k]) ?? k;
  function applyLang() {
    document.documentElement.lang = lang === 'en' ? 'en' : 'zh-CN';
    $$('[data-i18n]').forEach((el) => {
      const v = lang === 'en' ? window.I18N_EN[el.dataset.i18n] : zh[el.dataset.i18n];
      if (v !== undefined) el.innerHTML = v;
    });
    $('#lang').textContent = lang === 'en' ? '中文' : 'EN';
    document.title = lang === 'en' ? 'Vornel — a small, native, end-to-end encrypted remote connection app' : 'Vornel — 小巧、原生、端到端加密的远程连接工具';
    renderSchemes();
    renderRelease();
  }
  $('#lang').addEventListener('click', () => {
    lang = lang === 'en' ? 'zh' : 'en';
    store.set('vornel.lang', lang);
    applyLang();
  });

  // ------------------------------------------------------------ 主题
  let theme = store.get('vornel.theme') || 'ink';
  function applyTheme() {
    document.documentElement.dataset.theme = theme;
    $('meta[name="theme-color"]').content = theme === 'paper' ? '#FBFAF7' : '#0E0E10';
    if (scheme === 'auto') paintTerminal();
    renderSchemes();
  }
  $('#theme').addEventListener('click', () => {
    theme = theme === 'ink' ? 'paper' : 'ink';
    store.set('vornel.theme', theme);
    applyTheme();
  });

  // ------------------------------------------------------------ 终端配色（与应用内一致）
  const A = (n, b) => ({ n, b });
  const SCHEMES = {
    ink: { name: 'Ink', bg: '#0B0B0D', fg: '#D7D7DC', cur: '#8EA2FF', ...A(['#1A1A1E', '#F7768E', '#9ECE6A', '#E0AF68', '#7AA2F7', '#BB9AF7', '#7DCFFF', '#C8C8D0'], ['#62626C']) },
    paper: { name: 'Paper', bg: '#FFFFFF', fg: '#2A2925', cur: '#4353D1', ...A(['#2A2925', '#C2334D', '#3F7D20', '#9A6700', '#3557C9', '#8E44AD', '#0E7490', '#625F59'], ['#8F8B84']) },
    dracula: { name: 'Dracula', bg: '#282A36', fg: '#F8F8F2', cur: '#F8F8F2', ...A(['#21222C', '#FF5555', '#50FA7B', '#F1FA8C', '#BD93F9', '#FF79C6', '#8BE9FD', '#F8F8F2'], ['#6272A4']) },
    nord: { name: 'Nord', bg: '#2E3440', fg: '#D8DEE9', cur: '#D8DEE9', ...A(['#3B4252', '#BF616A', '#A3BE8C', '#EBCB8B', '#81A1C1', '#B48EAD', '#88C0D0', '#E5E9F0'], ['#4C566A']) },
    'one-dark': { name: 'One Dark', bg: '#282C34', fg: '#ABB2BF', cur: '#528BFF', ...A(['#282C34', '#E06C75', '#98C379', '#E5C07B', '#61AFEF', '#C678DD', '#56B6C2', '#ABB2BF'], ['#5C6370']) },
    'tokyo-night': { name: 'Tokyo Night', bg: '#1A1B26', fg: '#C0CAF5', cur: '#C0CAF5', ...A(['#15161E', '#F7768E', '#9ECE6A', '#E0AF68', '#7AA2F7', '#BB9AF7', '#7DCFFF', '#A9B1D6'], ['#414868']) },
    'gruvbox-dark': { name: 'Gruvbox Dark', bg: '#282828', fg: '#EBDBB2', cur: '#EBDBB2', ...A(['#282828', '#CC241D', '#98971A', '#D79921', '#458588', '#B16286', '#689D6A', '#A89984'], ['#928374']) },
    'catppuccin-mocha': { name: 'Catppuccin Mocha', bg: '#1E1E2E', fg: '#CDD6F4', cur: '#F5E0DC', ...A(['#45475A', '#F38BA8', '#A6E3A1', '#F9E2AF', '#89B4FA', '#F5C2E7', '#94E2D5', '#BAC2DE'], ['#585B70']) },
    monokai: { name: 'Monokai', bg: '#272822', fg: '#F8F8F2', cur: '#F8F8F0', ...A(['#272822', '#F92672', '#A6E22E', '#F4BF75', '#66D9EF', '#AE81FF', '#A1EFE4', '#F8F8F2'], ['#75715E']) },
    'solarized-dark': { name: 'Solarized Dark', bg: '#002B36', fg: '#839496', cur: '#93A1A1', ...A(['#073642', '#DC322F', '#859900', '#B58900', '#268BD2', '#D33682', '#2AA198', '#EEE8D5'], ['#586E75']) },
    'solarized-light': { name: 'Solarized Light', bg: '#FDF6E3', fg: '#657B83', cur: '#586E75', ...A(['#073642', '#DC322F', '#859900', '#B58900', '#268BD2', '#D33682', '#2AA198', '#EEE8D5'], ['#93A1A1']) },
    'github-light': { name: 'GitHub Light', bg: '#FFFFFF', fg: '#24292F', cur: '#0969DA', ...A(['#24292F', '#CF222E', '#116329', '#4D2D00', '#0969DA', '#8250DF', '#1B7C83', '#6E7781'], ['#57606A']) },
  };
  const ORDER = ['auto', 'dracula', 'nord', 'one-dark', 'tokyo-night', 'gruvbox-dark', 'catppuccin-mocha', 'monokai', 'solarized-dark', 'solarized-light', 'github-light'];
  let scheme = store.get('vornel.scheme') || 'auto';
  const cur = () => SCHEMES[scheme === 'auto' ? theme : scheme];

  function paintTerminal() {
    const s = cur();
    const body = $('.win-body');
    body.style.background = s.bg;
    body.style.color = s.fg;
    $('#win').style.background = s.bg;
    document.documentElement.style.setProperty('--t-red', s.n[1]);
    document.documentElement.style.setProperty('--t-green', s.n[2]);
    document.documentElement.style.setProperty('--t-yellow', s.n[3]);
    document.documentElement.style.setProperty('--t-blue', s.n[4]);
    document.documentElement.style.setProperty('--t-mag', s.n[5]);
    document.documentElement.style.setProperty('--t-cyan', s.n[6]);
    document.documentElement.style.setProperty('--t-dim', s.b[0]);
    document.documentElement.style.setProperty('--t-cur', s.cur);
    document.documentElement.style.setProperty('--t-bg', s.bg);
  }

  function setScheme(id) {
    scheme = id;
    store.set('vornel.scheme', id);
    paintTerminal();
    renderSchemes();
  }

  function renderSchemes() {
    const hero = $('#hero-schemes');
    $$('.sw', hero).forEach((b) => b.remove());
    ORDER.forEach((id) => {
      const s = id === 'auto' ? SCHEMES[theme] : SCHEMES[id];
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'sw';
      b.setAttribute('role', 'radio');
      b.setAttribute('aria-checked', String(id === scheme));
      b.innerHTML = `<span class="chip" style="background:${s.bg};color:${s.n[2]}">Aa</span>${id === 'auto' ? t('dyn.auto') : s.name}`;
      b.addEventListener('click', () => setScheme(id));
      hero.appendChild(b);
    });
    const pal = $('#palette');
    pal.innerHTML = '';
    pal.setAttribute('role', 'radiogroup');
    ORDER.forEach((id) => {
      const s = id === 'auto' ? SCHEMES[theme] : SCHEMES[id];
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'pcard';
      b.setAttribute('role', 'radio');
      b.setAttribute('aria-checked', String(id === scheme));
      b.innerHTML =
        `<div class="pv" style="background:${s.bg};color:${s.fg}">` +
        `<span style="color:${s.n[2]}">user@host</span>:<span style="color:${s.n[4]}">~</span>$ ls<br>` +
        `<span style="color:${s.n[4]}">src</span>  <span style="color:${s.n[2]}">run.sh</span>  <span style="color:${s.n[5]}">logo.png</span>` +
        `<div class="row">${s.n.slice(1, 7).map((c) => `<i style="background:${c}"></i>`).join('')}</div></div>` +
        `<span class="nm">${id === 'auto' ? t('dyn.auto') : s.name}</span>`;
      b.addEventListener('click', () => {
        setScheme(id);
        $('#top').scrollIntoView({ behavior: reduced ? 'auto' : 'smooth' });
      });
      pal.appendChild(b);
    });
  }

  // ------------------------------------------------------------ 终端演示
  const C = (v, c) => ({ text: v, color: c });
  const BADGE = (label, color) => ({ badge: label, color });
  const SCRIPT = [
    { line: [BADGE('···', 'dim'), C('connecting deploy@web-01:22', 'fg')] },
    { line: [BADGE('KEY', 'green'), C('web-01:22  SHA256:3kPz…Q9a', 'fg')] },
    { line: [BADGE('SSH', 'blue'), C('connected deploy@web-01:22', 'fg')], pause: 500 },
    { cmd: 'uptime' },
    { out: [C(' 14:02:11 up 42 days,  3:17,  1 user,  load average: 0.21, 0.18, 0.12', 'fg')] },
    { cmd: "docker ps --format '{{.Names}}\\t{{.Status}}'" },
    { out: [C('api       ', 'cyan'), C('Up 3 days (healthy)', 'green')] },
    { out: [C('worker    ', 'cyan'), C('Up 3 days', 'green')] },
    { out: [C('postgres  ', 'cyan'), C('Up 6 weeks', 'green')] },
    { cmd: 'git -C /srv/app log --oneline -3' },
    { out: [C('9f2c1ab ', 'yellow'), C('ship split view & broadcast input', 'fg')] },
    { out: [C('41d07e3 ', 'yellow'), C('encrypt sync with XChaCha20-Poly1305', 'fg')] },
    { out: [C('c0ffee1 ', 'yellow'), C('hello, world', 'fg')] },
    { cmd: 'echo "Small. Native. Yours."' },
    { out: [C('Small. Native. Yours.', 'mag')] },
  ];
  const colorVar = { fg: 'inherit', dim: 'var(--t-dim)', green: 'var(--t-green)', blue: 'var(--t-blue)', cyan: 'var(--t-cyan)', yellow: 'var(--t-yellow)', mag: 'var(--t-mag)', red: 'var(--t-red)' };
  const esc = (s) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]);
  const prompt = () => `<span style="color:var(--t-green)">deploy@web-01</span>:<span style="color:var(--t-blue)">~</span>$ `;
  const seg = (p) =>
    p.badge
      ? `<span class="badge" style="color:${colorVar[p.color]};background:color-mix(in srgb, ${colorVar[p.color]} 16%, transparent)">${p.badge}</span>`
      : `<span style="color:${colorVar[p.color]}">${esc(p.text)}</span>`;
  const out = $('#term-out');
  let html = '';
  const caret = '<span class="caret" style="background:var(--t-cur)"></span>';
  const render = (tail = '') => (out.innerHTML = html + tail + caret);
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

  async function runScript() {
    if (reduced) {
      SCRIPT.forEach((s) => {
        if (s.line || s.out) html += (s.line || s.out).map(seg).join('') + '\n';
        if (s.cmd) html += prompt() + esc(s.cmd) + '\n';
      });
      html += prompt();
      render();
      return;
    }
    for (;;) {
      html = '';
      for (const s of SCRIPT) {
        if (s.line || s.out) {
          html += (s.line || s.out).map(seg).join('') + '\n';
          render();
          await sleep(s.pause ?? (s.out ? 70 : 260));
        } else if (s.cmd) {
          html += prompt();
          let typed = '';
          for (const ch of s.cmd) {
            typed += ch;
            render(esc(typed));
            await sleep(28 + Math.random() * 55);
          }
          await sleep(220);
          html += esc(s.cmd) + '\n';
          render();
          await sleep(180);
        }
      }
      html += prompt();
      render();
      await sleep(5200);
    }
  }

  // ------------------------------------------------------------ 资源栏
  let cpu = 12;
  let mem = 1.9;
  function tickStats() {
    cpu = Math.max(3, Math.min(96, cpu + (Math.random() - 0.45) * 18));
    mem = Math.max(1.4, Math.min(6.9, mem + (Math.random() - 0.5) * 0.25));
    $('#s-cpu').textContent = `${cpu.toFixed(0)}%`;
    $('#s-cpu-bar').style.width = `${cpu}%`;
    $('#s-cpu').parentElement.classList.toggle('warn', cpu >= 70);
    $('#s-mem').textContent = `${mem.toFixed(1)}G/7.7G`;
    $('#s-mem-bar').style.width = `${(mem / 7.7) * 100}%`;
    const l = (cpu / 100) * 4;
    $('#s-load').textContent = `${l.toFixed(2)} ${(l * 0.8 + 0.1).toFixed(2)} ${(l * 0.6 + 0.08).toFixed(2)}`;
    $('#s-rx').textContent = `${(Math.random() * 3 + 0.2).toFixed(1)}M/s`;
    $('#s-tx').textContent = `${Math.round(Math.random() * 180 + 20)}K/s`;
  }

  // ------------------------------------------------------------ 分屏同步输入
  const quad = $('#quad');
  const panes = $$('.pane pre', quad);
  const paneBoxes = $$('.pane', quad);
  const qPrompt = (i) => `<span style="color:#F7768E">root@web-0${i + 1}</span>:<span style="color:#7AA2F7">~</span># `;
  const QCMDS = ['apt install -y nginx', 'nginx -t && nginx -s reload', 'nginx -v'];
  const QOUT = [
    (i) => `<span style="color:#9a9aa3">Setting up nginx (1.26.${i}) …</span>`,
    () => `<span style="color:#9ECE6A">syntax is ok · reloaded</span>`,
    () => `nginx version: nginx/1.26.3`,
  ];
  async function runQuad() {
    const bc = () => $('#bc-toggle').checked;
    for (;;) {
      const buf = panes.map((_, i) => qPrompt(i));
      panes.forEach((p, i) => (p.innerHTML = buf[i] + caret));
      paneBoxes.forEach((p, i) => p.classList.toggle('focus', i === 0));
      for (let c = 0; c < QCMDS.length; c++) {
        const cmd = QCMDS[c];
        const targets = bc() ? [0, 1, 2, 3] : [0];
        let typed = '';
        for (const ch of cmd) {
          typed += ch;
          targets.forEach((i) => (panes[i].innerHTML = buf[i] + esc(typed) + caret));
          await sleep(reduced ? 0 : 40 + Math.random() * 40);
        }
        await sleep(240);
        targets.forEach((i) => {
          buf[i] += esc(cmd) + '\n' + QOUT[c](i + 1) + '\n' + qPrompt(i);
          panes[i].innerHTML = buf[i] + caret;
        });
        await sleep(600);
      }
      await sleep(2600);
    }
  }
  $('#bc-toggle').addEventListener('change', (e) => quad.classList.toggle('bc', e.target.checked));
  quad.classList.toggle('bc', $('#bc-toggle').checked);

  // ------------------------------------------------------------ 主机卡片
  function renderHosts() {
    const I = window.OS_ICONS || {};
    const hosts = [
      ['web-01', 'ssh, deploy', 'ubuntu', true],
      ['db-primary', 'ssh, postgres', 'debian', true],
      ['edge-tokyo', 'mosh, root', 'alpine', false],
      ['build-runner', 'ssh, ci', 'fedora', false],
      ['nas', 'ssh, admin', 'freebsd', false],
      ['legacy-erp', 'ssh, root', 'centos', false],
      ['k8s-node-3', 'ssh, core', 'rocky', false],
      ['pi-garage', 'ssh, pi', 'raspbian', false],
    ];
    $('#hostgrid').innerHTML = hosts
      .map(([name, sub, os, live], i) => {
        const ic = I[os];
        const svg = ic ? `<svg viewBox="0 0 24 24" width="21" height="21" aria-hidden="true"><path fill="currentColor" d="${ic.path}"/></svg>` : '';
        return `<div class="hcard reveal" style="--d:${i * 60}ms"><span class="tile" style="background:${ic ? ic.bg : 'var(--acc)'};color:${ic ? ic.fg : '#fff'}" title="${ic ? ic.name : ''}">${svg}${live ? '<span class="live"></span>' : ''}</span><span><b>${name}</b><small>${sub}</small></span></div>`;
      })
      .join('');
    const apple = I.macos;
    if (apple) $('#apple').setAttribute('d', apple.path);
  }

  // ------------------------------------------------------------ 密文滚动
  function cipherLoop() {
    const el = $('#cipher');
    const hex = '0123456789abcdef';
    setInterval(() => {
      let s = '';
      for (let i = 0; i < 24; i++) s += hex[(Math.random() * 16) | 0] + (i % 4 === 3 && i < 23 ? ' ' : '');
      el.textContent = s;
    }, 140);
  }

  // ------------------------------------------------------------ 下载
  let release = null;
  const isMac = /Mac/.test(navigator.platform) || /Macintosh/.test(navigator.userAgent);
  const isWin = /Win/.test(navigator.platform);
  const mb = (b) => `${(b / 1048576).toFixed(1)} MB`;
  function renderRelease() {
    const label = $('#dl-main-label');
    const main = $('#dl-main');
    if (isMac) {
      label.textContent = t('dyn.dlMac');
      main.href = '/api/download/mac-arm';
      $('#dl-mac-arm').classList.add('mine');
    } else if (isWin) {
      label.textContent = t('dyn.dlWin');
      main.href = '/api/download/windows';
      $('#dl-win').classList.add('mine');
    } else {
      label.textContent = t('dyn.dl');
    }
    if (!release || !release.ok) return;
    $('#ver-pill').textContent = `v${release.version}`;
    $('#dl-version').textContent = `v${release.version}`;
    if (release.publishedAt) {
      const d = new Date(release.publishedAt);
      $('#dl-date').textContent = ` · ${t('dyn.released')} ${d.toLocaleDateString(lang === 'en' ? 'en-US' : 'zh-CN')}`;
    }
    const a = release.assets;
    if (a.macArm) $('#sz-mac-arm').textContent = `.dmg · ${mb(a.macArm.size)}`;
    if (a.macIntel) $('#sz-mac-intel').textContent = `.dmg · ${mb(a.macIntel.size)}`;
    if (a.winExe) $('#sz-win').textContent = `.exe · ${mb(a.winExe.size)}`;
  }
  async function loadRelease() {
    try {
      const r = await fetch('/api/release');
      release = await r.json();
    } catch {
      release = null;
    }
    renderRelease();
  }

  // ------------------------------------------------------------ 动效
  function effects() {
    const nav = $('#nav');
    const win = $('#win');
    const onScroll = () => {
      nav.classList.toggle('scrolled', scrollY > 8);
      win.classList.toggle('flat', scrollY > 120);
    };
    addEventListener('scroll', onScroll, { passive: true });
    onScroll();

    // 首屏光晕跟随鼠标
    const hero = $('.hero');
    hero.addEventListener('pointermove', (e) => {
      const r = hero.getBoundingClientRect();
      hero.style.setProperty('--mx', `${((e.clientX - r.left) / r.width) * 100}%`);
      hero.style.setProperty('--my', `${((e.clientY - r.top) / r.height) * 100}%`);
    });

    // 出现动画（同一批依次出现）
    const io = new IntersectionObserver(
      (entries) => {
        let n = 0;
        entries.forEach((en) => {
          if (!en.isIntersecting) return;
          const el = en.target;
          if (!el.style.getPropertyValue('--d')) el.style.setProperty('--d', `${(n++ % 6) * 70}ms`);
          el.classList.add('in');
          io.unobserve(el);
          if (el.matches('.num')) countUp(el);
          $$('.sizes', el).forEach((s) => s.classList.add('in'));
        });
      },
      { threshold: 0.12, rootMargin: '0px 0px -40px 0px' },
    );
    $$('.reveal').forEach((el) => io.observe(el));

    // 卡片：光斑跟随与轻微倾斜
    $$('.tilt').forEach((card) => {
      card.addEventListener('pointermove', (e) => {
        const r = card.getBoundingClientRect();
        const x = (e.clientX - r.left) / r.width;
        const y = (e.clientY - r.top) / r.height;
        card.style.setProperty('--x', `${x * 100}%`);
        card.style.setProperty('--y', `${y * 100}%`);
        if (!reduced) {
          card.style.setProperty('--tx', `${(x - 0.5) * 6}deg`);
          card.style.setProperty('--ty', `${(0.5 - y) * 6}deg`);
        }
      });
      card.addEventListener('pointerleave', () => {
        card.style.setProperty('--tx', '0deg');
        card.style.setProperty('--ty', '0deg');
      });
    });

    // 主按钮：磁吸
    $$('.magnetic').forEach((b) => {
      b.addEventListener('pointermove', (e) => {
        if (reduced) return;
        const r = b.getBoundingClientRect();
        b.style.transform = `translate(${(e.clientX - r.left - r.width / 2) * 0.15}px, ${(e.clientY - r.top - r.height / 2) * 0.25}px)`;
      });
      b.addEventListener('pointerleave', () => (b.style.transform = ''));
    });
  }

  function countUp(el) {
    const b = $('b', el);
    const target = parseFloat(b.dataset.count);
    if (reduced || !target) return;
    const dec = String(b.dataset.count).includes('.') ? 1 : 0;
    const t0 = performance.now();
    const step = (now) => {
      const p = Math.min(1, (now - t0) / 1100);
      b.textContent = (target * (1 - Math.pow(1 - p, 3))).toFixed(dec);
      if (p < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  }

  // ------------------------------------------------------------ 启动
  renderHosts();
  applyTheme();
  applyLang();
  paintTerminal();
  effects();
  void runScript();
  void runQuad();
  setInterval(tickStats, 2000);
  if (!reduced) cipherLoop();
  void loadRelease();
})();
