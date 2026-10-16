// wqq-float 悬浮窗前端：渲染/交互/可见矩形上报（方案一' 穿透补偿）。
// 安全规则：帖子数据一律 textContent 填充，禁止 innerHTML（RSS 源内容不可信）。

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const ball = $('ball'), bar = $('bar'), col = $('col'), card = $('card');
const titleEl = $('title'), timeEl = $('time'), menu = $('menu');

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
listen('new-threads', (e) => {
  const threads = e.payload || [];
  if (threads.length > 0) render(threads[0]);
});

listen('conn-state', (e) => {
  connOk = !!(e.payload && e.payload.ok);
  ball.classList.toggle('warn', !connOk);
  refreshTime();
});

// ---------- 点击 ----------
ball.addEventListener('click', () => invoke('open_url', { url: forumBase }));
bar.addEventListener('click', () => {
  if (current) invoke('open_url', { url: current.url });
});

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
  for (const el of [ball, col]) {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) {
      rects.push([r.left * dpr, r.top * dpr, r.width * dpr, r.height * dpr]);
    }
  }
  return rects;
}
function reportRects() {
  invoke('report_ui_rects', { rects: collectRects() }).catch(() => {});
}
setInterval(reportRects, 250);
window.addEventListener('resize', reportRects);

// ---------- 初始化 ----------
(async () => {
  try {
    const cfg = await invoke('get_settings');
    forumBase = cfg.forum_base || forumBase;
  } catch (_) { /* 默认值兜底 */ }
  reportRects();
})();
