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
    });
    show('已保存（若更换了论坛/数据源，将重新全量拉取）', 'ok');
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

load();
