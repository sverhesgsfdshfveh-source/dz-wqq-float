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

// ---------- 自动升级 ----------
// check_update：查 GitHub Release 最新版；install_update：下载并替换自身后自动重启
let updateInfo = null;

$('check-update').addEventListener('click', async () => {
  const btn = $('check-update'), st = $('upd-status');
  btn.disabled = true; btn.textContent = '检查中…';
  st.className = 'upd-status'; st.textContent = '';
  try {
    const info = await invoke('check_update');
    updateInfo = info;
    if (info.has_update) {
      st.className = 'upd-status ok';
      st.textContent = `发现新版本 v${info.latest}（${(info.size / 1048576).toFixed(1)} MB）`;
      $('do-update').hidden = false;
    } else {
      st.textContent = `已是最新版本 v${info.current}`;
      $('do-update').hidden = true;
    }
  } catch (e) {
    st.className = 'upd-status err';
    st.textContent = `检查失败：${e}`;
    $('do-update').hidden = true;
  } finally {
    btn.disabled = false; btn.textContent = '检查更新';
  }
});

$('do-update').addEventListener('click', async () => {
  if (!updateInfo) return;
  const st = $('upd-status');
  $('do-update').disabled = true;
  $('check-update').disabled = true;
  st.className = 'upd-status';
  st.textContent = '开始下载…';
  try {
    await invoke('install_update', {
      assetUrl: updateInfo.asset_url,
      expectedSize: updateInfo.size,
    });
    // 成功的正常路径：Rust 侧已启动替换脚本，本进程即将退出并重启
    st.textContent = '更新就绪，应用即将重启…';
  } catch (e) {
    st.className = 'upd-status err';
    st.textContent = `更新失败：${e}`;
    $('do-update').disabled = false;
    $('check-update').disabled = false;
  }
});

listen('update-progress', (e) => {
  const p = e.payload || {};
  if (p.retry) {
    // 加速代理失败 → Rust 侧自动回退直连重试
    $('upd-status').textContent = '加速通道失败，切换直连重试…';
    return;
  }
  const mb = (n) => (n / 1048576).toFixed(1);
  if (p.total > 0) {
    const pct = Math.min(100, Math.round((p.downloaded / p.total) * 100));
    $('upd-status').textContent = `下载中 ${pct}%（${mb(p.downloaded)}/${mb(p.total)} MB）`;
  } else {
    $('upd-status').textContent = `已下载 ${mb(p.downloaded)} MB`;
  }
});

// about 行版本号动态填充（升级后不会显示旧版本）
invoke('get_version')
  .then((v) => {
    document.querySelector('.about').textContent = `wqq-float v${v} · 微趣趣线报网桌面通知`;
  })
  .catch(() => {});

load();

// 窗口高度自适应：按 body 实测内容高度（逻辑像素）上报 Rust 侧 resize。
// 用 body 而不是 documentElement：内容比视口矮时 documentElement.scrollHeight 会
// 返回视口高度（永远测不出"缩小"）。失败静默（保持初始高度可用）。
function fitHeight() {
  const h = Math.ceil(document.body.scrollHeight) + 2;
  invoke('resize_settings', { height: h }).catch(() => {});
}
window.addEventListener('load', () => requestAnimationFrame(fitHeight));

load();
