// wqq-float 悬浮窗前端：渲染/交互/可见矩形上报（方案一' 穿透补偿）。
// 安全规则：帖子数据一律 textContent 填充，禁止 innerHTML（RSS 源内容不可信）。

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const ball = $('ball'), bar = $('bar'), barwrap = $('barwrap'), card = $('card');
const titleEl = $('title'), timeEl = $('time'), menu = $('menu'), cmenu = $('cmenu');
const progress = $('progress');

let forumBase = 'https://bbs.weiququ.cn';
let current = null;          // 当前展示的 Thread
let connOk = true;

// ---------- 相对时间 ----------
function relTime(ts) {
  const d = Math.max(0, Date.now() / 1000 - ts);
  if (d < 60) return '刚刚';
  if (d < 3600) return Math.floor(d / 60) + '分钟前';
  if (d < 86400) return Math.floor(d / 3600) + '小时前';
  return Math.floor(d / 86400) + '天前';
}
function refreshTime() {
  if (!current) return;
  timeEl.textContent = connOk ? relTime(current.dateline) : '重连中…';
  $('card-time').textContent = relTime(current.dateline);
}
setInterval(refreshTime, 30000);

// ---------- 渲染 ----------
function render(t) {
  current = t;
  // 长条：新帖标题向下滚动切换（300ms 动画）
  titleEl.textContent = t.subject;
  const tk = $('ticker');
  tk.classList.remove('slide');
  void tk.offsetWidth; // 重启动画
  tk.classList.add('slide');
  // 进度线扫光
  progress.classList.remove('sweep');
  void progress.offsetWidth;
  progress.classList.add('sweep');
  refreshTime();
  // 展开卡（textContent，永不 innerHTML）
  $('card-title').textContent = t.subject;
  $('card-author').textContent = t.author || '匿名';
  $('card-forum').textContent = t.forumname || '';
  $('card-replies').textContent = t.replies;
  $('card-views').textContent = t.views;
  $('card-summary').textContent = t.summary || '（该帖无摘要内容）';
  // 标题宽度变化 → 重新贴合窗口尺寸（卡片展开中不动，收起后由 mouseleave 缩回）
  if (!barwrap.matches(':hover')) setTimeout(() => fitWindow(), 80);
}

// ---------- 事件 ----------
// 提示音已移到 Rust 侧播放（WebView2 autoplay 受限）；payload 为 {threads, initial}
listen('new-threads', (e) => {
  const p = e.payload || {};
  const threads = p.threads || [];
  if (threads.length > 0) render(threads[0]);
});

listen('conn-state', (e) => {
  connOk = !!(e.payload && e.payload.ok);
  ball.classList.toggle('warn', !connOk);
  refreshTime();
});

// ---------- 点击 ----------
// 菜单开着时点球/条：只收起菜单（避免"关菜单"顺手打开网站/帖子的混乱）；
// moved=true 表示刚结束一次拖动，忽略其后的 click
ball.addEventListener('click', () => {
  if (moved) { moved = false; return; }
  if (!menu.hidden) { closeMenus(); return; }
  invoke('open_url', { url: forumBase });
});
bar.addEventListener('click', () => {
  if (moved) { moved = false; return; }
  if (!menu.hidden) { closeMenus(); return; }
  if (current) invoke('open_url', { url: current.url });
});

// ---------- 拖动（自绘实现：不用系统 HTCAPTION，避免 Aero Snap 把窗口压变形/拖到边缘弹回） ----------
// 位移超过 4px 才算拖动（区分单击）；拖动期间 set_dragging 冻结穿透判定
let dragging = false, moved = false, lastX = 0, lastY = 0, startX = 0, startY = 0;

ball.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  dragging = true;
  moved = false;
  startX = lastX = e.screenX;
  startY = lastY = e.screenY;
  invoke('set_dragging', { active: true }).catch(() => {});
});
window.addEventListener('mousemove', (e) => {
  if (!dragging) return;
  if (!moved && Math.hypot(e.screenX - startX, e.screenY - startY) < 4) return;
  const dx = e.screenX - lastX;
  const dy = e.screenY - lastY;
  lastX = e.screenX;
  lastY = e.screenY;
  moved = true;
  if (dx !== 0 || dy !== 0) invoke('move_window_by', { dx, dy }).catch(() => {});
});
function endDrag() {
  if (!dragging) return;
  dragging = false;
  invoke('set_dragging', { active: false }).catch(() => {});
}
window.addEventListener('mouseup', endDrag);
window.addEventListener('blur', endDrag);

// 球右键自绘菜单：贴球下方居中定位（稳定，不随右键点漂移）
// 统一关闭所有自绘菜单（球功能菜单 + 卡片内容菜单）
function closeMenus() {
  let changed = false;
  if (!menu.hidden) { menu.hidden = true; changed = true; }
  if (!cmenu.hidden) { cmenu.hidden = true; changed = true; }
  if (changed) {
    reportRects();
    // 缩回窗口（卡片仍展开时保持卡片所需高度，由 mouseleave 负责缩）
    if (!card.classList.contains('show')) fitWindow();
  }
}
ball.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  menu.hidden = false;
  const mw = menu.offsetWidth, mh = menu.offsetHeight;
  const br = ball.getBoundingClientRect();
  // 窗口先撑到能容下菜单（贴球下方），否则小窗口会把菜单裁掉
  const need = Math.max(baseSize().h, br.bottom + 6 + mh + 8);
  fitWindow(need);
  const left = Math.max(4, Math.min(br.left + br.width / 2 - mw / 2, window.innerWidth - mw - 4));
  menu.style.left = left + 'px';
  menu.style.top = (br.bottom + 6) + 'px';
  reportRects(); // 立即上报：菜单区域马上进入可交互判定区（否则穿透导致 hover/点击失效）
});
// 卡片内容右键菜单：复制 / 全选（提供自绘替代，屏蔽 WebView2 默认菜单）
card.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  cmenu.hidden = false;
  const mw = cmenu.offsetWidth, mh = cmenu.offsetHeight;
  // 窗口撑到能容下菜单（跟随鼠标位置）
  const need = Math.max(baseSize().h, e.clientY + mh + 8);
  fitWindow(need);
  cmenu.style.left = Math.max(4, Math.min(e.clientX, window.innerWidth - mw - 4)) + 'px';
  cmenu.style.top = Math.max(4, Math.min(e.clientY, window.innerHeight - mh - 4)) + 'px';
  reportRects();
});
cmenu.addEventListener('click', (e) => {
  const act = e.target.dataset && e.target.dataset.act;
  if (!act) return;
  if (act === 'copy') {
    try { document.execCommand('copy'); } catch (_) { /* 无选中内容时静默 */ }
  } else if (act === 'selectall') {
    const sel = window.getSelection();
    sel.removeAllRanges();
    const r = document.createRange();
    r.selectNodeContents(card);
    sel.addRange(r);
  }
  closeMenus();
});
// 打开原帖链接
$('card-open').addEventListener('click', (e) => {
  e.preventDefault();
  if (current) invoke('open_url', { url: current.url });
});
// 全局屏蔽 WebView2 默认右键菜单（Tauri 默认已禁，双保险）
document.addEventListener('contextmenu', (e) => e.preventDefault());
document.addEventListener('click', (e) => {
  if ((!menu.hidden && !menu.contains(e.target)) || (!cmenu.hidden && !cmenu.contains(e.target))) {
    closeMenus();
  }
});
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') closeMenus();
});
// 窗口失焦（点其他应用、或点穿透空白区后下层聚焦）→ 收起菜单，避免一直挂着
listen('float-blur', closeMenus);
menu.addEventListener('click', (e) => {
  const act = e.target.dataset && e.target.dataset.act;
  if (!act) return;
  closeMenus();
  if (act === 'forum') invoke('open_url', { url: forumBase });
  if (act === 'settings') invoke('open_settings').catch(() => {});
  if (act === 'quit') invoke('quit_app');
});

// ---------- 可见矩形上报（方案一' 穿透补偿核心） ----------
// 物理像素 = CSS 像素 × devicePixelRatio；坐标相对窗口客户区（= viewport）
function collectRects() {
  const dpr = window.devicePixelRatio || 1;
  const rects = [];
  const push = (r, extraBottom) => {
    if (r.width > 0 && r.height > 0) {
      rects.push([r.left * dpr, r.top * dpr, r.width * dpr, (r.height + (extraBottom || 0)) * dpr]);
    }
  };
  push(ball.getBoundingClientRect());
  // 条容器底部外扩 24px：覆盖条与卡片之间的悬停桥接带（与 CSS #barwrap::after 高度一致）
  push(barwrap.getBoundingClientRect(), 24);
  // 卡片展开时纳入判定，否则鼠标移到卡上会被判穿透
  push(card.getBoundingClientRect());
  // 菜单显示时纳入判定，否则鼠标移到菜单上会穿透（hover 与点击全部失效）
  if (!menu.hidden) push(menu.getBoundingClientRect());
  if (!cmenu.hidden) push(cmenu.getBoundingClientRect());
  return rects;
}
function reportRects() {
  invoke('report_ui_rects', { rects: collectRects() }).catch(() => {});
}
setInterval(reportRects, 250);
window.addEventListener('resize', reportRects);
// ---------- 窗口自适应尺寸（窗口贴合"球+条"组合；卡片展开时加高，收起后缩回） ----------
// 窗口高度只有球高量级 → 可拖到屏幕最顶/最底；浏览器大窗口的边界限制与 Snap 问题随之消失
const RESIZE_PAD = 20;   // 右/下余量（阴影/光晕）
const CARD_TOP = 88;     // 卡片顶在窗口内的 y（球高 80 + 间隙 8）
let shrinkTimer = null;

function baseSize() {
  const r = $('hud').getBoundingClientRect();
  // 宽度至少容纳卡片（min-width 360 + 左右边距），否则卡片会溢出窗口被裁
  return {
    w: Math.max(Math.ceil(r.width) + RESIZE_PAD, 380),
    h: Math.ceil(r.height) + RESIZE_PAD,
  };
}
function fitWindow(extraH) {
  const b = baseSize();
  const h = extraH ? Math.max(b.h, extraH) : b.h;
  invoke('resize_float', { width: b.w, height: h }).catch(() => {});
}

// 卡片展开：先撑开窗口再显示卡片（卡片已移出 barwrap，hover 用 JS 管理 + 120ms 宽限跨间隙）
let closeTimer = null;
function openCard() {
  clearTimeout(shrinkTimer);
  clearTimeout(closeTimer);
  const cardNeed = (card.scrollHeight || 0) + 28; // scrollHeight 不含 padding，补上下 14*2
  const need = CARD_TOP + Math.min(cardNeed, 300) + RESIZE_PAD;
  const b = baseSize();
  fitWindow(need);
  const avail = Math.max(120, Math.min(300, Math.max(need, b.h) - CARD_TOP - 8));
  card.style.setProperty('--card-max', avail + 'px');
  card.classList.add('show');
  reportRects();
}
function closeCard() {
  clearTimeout(closeTimer);
  card.classList.remove('show');
  reportRects();
  clearTimeout(shrinkTimer);
  shrinkTimer = setTimeout(() => fitWindow(), 280); // 等收起动画结束再缩窗口
}
barwrap.addEventListener('mouseenter', openCard);
barwrap.addEventListener('mouseleave', () => {
  clearTimeout(closeTimer);
  closeTimer = setTimeout(closeCard, 120); // 宽限：允许鼠标穿过条与卡片的间隙
});
card.addEventListener('mouseenter', () => {
  clearTimeout(closeTimer);
  clearTimeout(shrinkTimer);
});
card.addEventListener('mouseleave', closeCard);

// ---------- 初始化 ----------
// 读取配置：论坛地址 + 主题（dark 纯黑 / system 跟随系统）
async function applyConfig() {
  try {
    const cfg = await invoke('get_settings');
    forumBase = cfg.forum_base || forumBase;
    document.body.className = cfg.theme === 'system' ? 'theme-system' : 'theme-dark';
  } catch (_) { /* 默认值兜底 */ }
}
// 设置页保存后刷新主题
listen('settings-changed', applyConfig);

(async () => {
  await applyConfig();
  reportRects();
  // 窗口贴合组合尺寸（加载后立即 + 布局稳定后校正一次）
  fitWindow();
  setTimeout(() => fitWindow(), 400);
  // 首轮 poll 常早于本页就绪（emit 事件丢失），主动补拉一次
  try {
    const t = await invoke('get_latest_thread');
    if (t) render(t);
  } catch (_) { /* 首轮无数据则等事件 */ }
})();
