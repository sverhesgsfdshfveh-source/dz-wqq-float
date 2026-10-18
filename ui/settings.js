// 设置窗：读配置 → 表单 → 保存（save_settings 内部会重置水位并落盘）
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const autostartApi = window.__TAURI__.autostart;   // tauri-plugin-autostart JS API

const $ = (id) => document.getElementById(id);

async function load() {
  try {
    const cfg = await invoke('get_settings');
    $('forum-base').value = cfg.forum_base;
    $('source').value = cfg.source === 'rss' ? 'rss' : 'wqq';
    $('interval').value = cfg.poll_interval_secs;
    $('theme').value = cfg.theme === 'system' ? 'system' : 'dark';
  } catch (e) {
    show('读取配置失败: ' + e, 'err');
  }
  try {
    $('autostart').checked = await autostartApi.isEnabled();
  } catch (_) {
    $('autostart').checked = false;
  }
}

function show(text, cls) {
  const m = $('msg');
  m.textContent = text;
  m.className = cls || '';
}

listen('conn-state', (e) => {
  const p = e.payload || {};
  const via = $('via');
  if (p.via === 'rss') {
    via.textContent = p.ok
      ? 'RSS 通道（分钟级）'
      : '连接失败，重试中…';
    via.className = p.ok ? '' : 'warn';
  } else {
    via.textContent = p.ok
      ? '微趣趣接口（秒级）'
      : '微趣趣接口异常，重试中（连败将自动切 RSS 兜底）';
    via.className = p.ok ? '' : 'warn';
  }
});

$('save').addEventListener('click', async () => {
  try {
    await invoke('save_settings', {
      forumBase: $('forum-base').value,
      source: $('source').value,
      pollIntervalSecs: Number($('interval').value) || 5,
      theme: $('theme').value,
    });
    show('已保存（若更换了论坛/数据源，将重新全量拉取）', 'ok');
    // 保存成功后自动收起设置窗（留 400ms 让"已保存"提示可见）。
    // 走 hide_settings（hide 保活）而非 close：销毁后重建会白屏，见 main.rs
    setTimeout(() => invoke('hide_settings').catch(() => {}), 400);
  } catch (e) {
    show('保存失败: ' + e, 'err');
  }
});

$('autostart').addEventListener('change', async (e) => {
  try {
    if (e.target.checked) {
      await autostartApi.enable();
    } else {
      await autostartApi.disable();
    }
    show('开机自启已' + (e.target.checked ? '开启' : '关闭'), 'ok');
  } catch (err) {
    show('自启设置失败: ' + err, 'err');
    e.target.checked = !e.target.checked;
  }
});

$('check-update').addEventListener('click', () => {
  invoke('open_url', { url: 'https://bbs.weiququ.cn/' }).catch((e) => show('打开失败: ' + e, 'err'));
});

// 窗口高度自适应：按 body 实测内容高度（逻辑像素）上报 Rust 侧 resize。
// 用 body 而不是 documentElement：内容比视口矮时 documentElement.scrollHeight 会
// 返回视口高度（永远测不出"缩小"）。失败静默（保持初始高度可用）。
function fitHeight() {
  const h = Math.ceil(document.body.scrollHeight) + 2;
  invoke('resize_settings', { height: h }).catch(() => {});
}
window.addEventListener('load', () => requestAnimationFrame(fitHeight));

load();
