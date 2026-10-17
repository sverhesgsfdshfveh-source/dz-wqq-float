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
  // 未读红点亮起（点击球/条后熄灭）
  ball.classList.add('unread');
  refreshTime();
  // 展开卡（textContent，永不 innerHTML）
  $('card-title').textContent = t.subject;
  $('card-author').textContent = t.author || '匿名';
  $('card-forum').textContent = t.forumname || '';
  $('card-replies').textContent = t.replies;
  $('card-views').textContent = t.views;
  $('card-summary').textContent = t.summary || '（该帖无摘要内容）';
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
// 菜单开着时点球/条：只收起菜单（避免"关菜单"顺手打开网站/帖子的混乱）
ball.addEventListener('click', () => {
  if (!menu.hidden) { closeMenus(); return; }
  ball.classList.remove('unread');
  invoke('open_url', { url: forumBase });
});
bar.addEventListener('click', () => {
  if (!menu.hidden) { closeMenus(); return; }
  ball.classList.remove('unread');
  if (current) invoke('open_url', { url: current.url });
});

// ---------- 拖动 ----------
// 不用 data-tauri-drag-region：注入脚本会在 mousedown 立即 start_dragging（还需 ACL 权限），
// 且系统拖动循环会被穿透切换打断。这里改为阈值拖动：位移超 4px 才进入系统拖动，
// 按下期间通过 set_dragging 让后端冻结穿透判定，保证拖动不中断。
let pending = null; // 按下起点（屏幕坐标）
const DRAG_THRESHOLD = 4;

ball.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  pending = { x: e.screenX, y: e.screenY };
  invoke('set_dragging', { active: true }).catch(() => {});
});
window.addEventListener('mousemove', (e) => {
  if (!pending) return;
  if (Math.hypot(e.screenX - pending.x, e.screenY - pending.y) < DRAG_THRESHOLD) return;
  pending = null;
  invoke('start_drag').catch(() => {});
});
function endDrag() {
  pending = null;
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
  if (changed) reportRects();
}
ball.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  const br = ball.getBoundingClientRect();
  menu.hidden = false;
  const mw = menu.offsetWidth, mh = menu.offsetHeight;
  const left = Math.max(4, Math.min(br.left + br.width / 2 - mw / 2, window.innerWidth - mw - 4));
  // 下方空间不足时翻到球上方
  const top = (br.bottom + 6 + mh <= window.innerHeight - 4)
    ? br.bottom + 6
    : Math.max(4, br.top - mh - 6);
  menu.style.left = left + 'px';
  menu.style.top = top + 'px';
  reportRects(); // 立即上报：菜单区域马上进入可交互判定区（否则穿透导致 hover/点击失效）
});
// 卡片内容右键菜单：复制 / 全选（提供自绘替代，屏蔽 WebView2 默认菜单）
card.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  cmenu.hidden = false;
  const mw = cmenu.offsetWidth, mh = cmenu.offsetHeight;
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
  // 条容器底部外扩 14px：覆盖条与卡片之间的悬停桥接带
  push(barwrap.getBoundingClientRect(), 14);
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
// hover 展开/收起瞬间立即上报：卡片 bbox 变化不等 250ms 周期
barwrap.addEventListener('mouseenter', () => {
  // 卡片高度按窗口可用空间自适应（窗口被 Snap 压矮时也保证内容尽量完整）
  const top = barwrap.getBoundingClientRect().bottom + 12 + 8;
  const avail = Math.max(120, Math.min(300, window.innerHeight - top));
  card.style.setProperty('--card-max', avail + 'px');
  reportRects();
});
barwrap.addEventListener('mouseleave', reportRects);

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
  // 首轮 poll 常早于本页就绪（emit 事件丢失），主动补拉一次
  try {
    const t = await invoke('get_latest_thread');
    if (t) render(t);
  } catch (_) { /* 首轮无数据则等事件 */ }
})();
