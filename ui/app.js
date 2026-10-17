// wqq-float 悬浮窗前端：渲染/交互/可见矩形上报（方案一' 穿透补偿）。
// 安全规则：帖子数据一律 textContent 填充，禁止 innerHTML（RSS 源内容不可信）。

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const ball = $('ball'), bar = $('bar'), barwrap = $('barwrap'), card = $('card');
const titleEl = $('title'), timeEl = $('time'), menu = $('menu'), progress = $('progress');

let forumBase = 'https://bbs.weiququ.cn';
let current = null;          // 当前展示的 Thread
let connOk = true;

// ---------- 新帖提示音（音效提取自 ZCode 的 task-notification-pop） ----------
const notifyAudio = new Audio('assets/notify.mp3');
function playNotify() {
  try {
    notifyAudio.currentTime = 0; // 连续新帖时从头播
    notifyAudio.play().catch(() => { /* 自动播放被拒时静默 */ });
  } catch (_) { /* 音频不可用时静默 */ }
}

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
// opts.silent：初始化渲染（启动补拉 / 首轮存量帖）不播提示音
function render(t, opts) {
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
  if (!(opts && opts.silent)) playNotify();
}

// ---------- 事件 ----------
listen('new-threads', (e) => {
  const p = e.payload || {};
  const threads = p.threads || [];
  // initial=true 是启动首轮的存量帖（20 条历史），不响提示音
  if (threads.length > 0) render(threads[0], { silent: !!p.initial });
});

listen('conn-state', (e) => {
  connOk = !!(e.payload && e.payload.ok);
  ball.classList.toggle('warn', !connOk);
  refreshTime();
});

// ---------- 点击 ----------
ball.addEventListener('click', () => {
  ball.classList.remove('unread');
  invoke('open_url', { url: forumBase });
});
bar.addEventListener('click', () => {
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

// 球右键自绘菜单
ball.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  menu.hidden = false;
  menu.style.left = Math.max(0, e.clientX - menu.offsetWidth) + 'px';
  menu.style.top = (e.clientY + 6) + 'px';
});
document.addEventListener('click', (e) => {
  if (!menu.hidden && !menu.contains(e.target)) menu.hidden = true;
});
menu.addEventListener('click', (e) => {
  const act = e.target.dataset && e.target.dataset.act;
  if (!act) return;
  menu.hidden = true;
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
  return rects;
}
function reportRects() {
  invoke('report_ui_rects', { rects: collectRects() }).catch(() => {});
}
setInterval(reportRects, 250);
window.addEventListener('resize', reportRects);
// hover 展开/收起瞬间立即上报：卡片 bbox 变化不等 250ms 周期
barwrap.addEventListener('mouseenter', reportRects);
barwrap.addEventListener('mouseleave', reportRects);

// ---------- 初始化 ----------
(async () => {
  try {
    const cfg = await invoke('get_settings');
    forumBase = cfg.forum_base || forumBase;
  } catch (_) { /* 默认值兜底 */ }
  reportRects();
  // 首轮 poll 常早于本页就绪（emit 事件丢失），主动补拉一次（初始化渲染，不响提示音）
  try {
    const t = await invoke('get_latest_thread');
    if (t) render(t, { silent: true });
  } catch (_) { /* 首轮无数据则等事件 */ }
})();
