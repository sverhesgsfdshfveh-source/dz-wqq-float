# AGENTS.md — wqq-float 桌面悬浮通知客户端（灵魂文件）

> 本文件是 ZCode 处理 `wqq-float/` 目录任务时的自动加载指令（同 xb-mover/AGENTS.md 的定位）。
> **动手前先读**：[@docs/release-checklist.md](docs/release-checklist.md)（历轮根因记录，最省时间的排障索引）
> 与 [@docs/superpowers/specs/2026-10-16-wqq-float-design.md](docs/superpowers/specs/2026-10-16-wqq-float-design.md)（原始设计文档）。
> 服务端接口在另一仓库：`../DiscuzX5/wqq_notifier.php`（只读 JSON 接口，已登记二开保护清单）。

## 一句话

微趣趣线报网（bbs.weiququ.cn）的 Windows 桌面常驻「悬浮球 + 通知条」客户端：
Tauri 2 + Rust + 纯静态 HTML/CSS/JS（零构建链），单 exe 分发给论坛用户，
新帖秒级到达时滚动提示 + 播放提示音，点击直通帖子。

## 铁律（改代码前默念）

1. **改前端（ui/）也要重新编译**：`cargo build` 的 debug 产物同样把 `ui/` 资源嵌入 exe（实测），改完必须重新 build 再重启进程；`tauri.conf.json` 的窗口初始尺寸同理
2. **用户数据不能丢**：`config.json` 加字段必须带 `#[serde(default = ...)]`，否则旧配置反序列化失败会静默回落默认值，把用户设置全清掉（已有测试 `old_config_without_theme_still_loads`）
3. **安全规则**：帖子数据一律 `textContent` 填充，**禁止 innerHTML**（RSS 源内容不可信）
4. **窗口/穿透三处联动**：改布局后必须同步检查 `collectRects`（占比判定区）、`fitWindow`（窗口尺寸）、hover 链，漏一处就出现"点不到/被裁"
5. **改完即 commit**：本地 git 是唯一事实源；发布清单记录根因（新踩的坑追加进去）
6. **测试基线**：改 Rust 必须 `cargo test`（当前 21/21 全绿）才能 commit

## 架构总览

```
wqq-float/
├── ui/                        # 静态前端（frontendDist 直指，无构建链）
│   ├── index.html|app.css|app.js        # 悬浮窗：球+条+卡片+自绘菜单
│   ├── settings.html|settings.css|settings.js  # 设置窗
│   └── assets/logo.png（favicon 转透明）、notify.mp3（ZCode 提示音）
└── src-tauri/
    ├── src/
    │   ├── main.rs      # Builder/插件注册/托盘菜单/两窗口/on_window_event（位置记忆+失焦+设置窗 hide）
    │   ├── poller.rs    # 轮询循环 + 光标穿透 watcher（10ms）+ settle_geometry（clamp+右缘吸附）+ play_notify
    │   ├── source.rs    # ThreadSource：WqqApiSource（JSON 秒级）| RssSource（DZ 通用兜底）
    │   ├── config.rs    # Config{forum_base,source,poll_interval_secs,window_pos,theme} + ThemeKind/SourceKind
    │   ├── state.rs     # SharedState{watermark,ui_rects,drag_started,last_moved,latest,...}
    │   ├── commands.rs  # 全部 Tauri command（见下）
    │   └── log.rs       # 轻量日志（仅失败时写 %APPDATA%\cn.weiququ.float\log.txt）
    ├── capabilities/default.json        # ACL 权限
    └── tauri.conf.json  # 窗口初始 760x84（前端 fitWindow 会立刻校正）
```

### 运行形态

- 两个窗口：`float`（透明/无边框/置顶/跳过任务栏）+ `settings`（**启动时预创建隐藏**，见踩坑 #2）
- 两个后台任务：`run_poller`（数据）+ `run_cursor_watcher`（穿透判定）
- 单实例插件：重复启动聚焦已有窗口；托盘常驻（打开论坛/设置/开机自启/退出）

### 事件协议（Rust → 前端）

| 事件 | payload | 用途 |
|---|---|---|
| `new-threads` | `{threads:[...], initial:bool}` | 新帖渲染；`initial=true` 是首轮 20 条存量（前端不播提示音） |
| `conn-state` | `{ok, via}` | 球警示圈 + 设置页通道状态 |
| `float-blur` | `()` | float 窗口失焦 → 前端收起自绘菜单（穿透区收不到点击，只能靠焦点事件） |
| `settings-changed` | `()` | 设置保存后 → 悬浮窗重读配置（主题即时刷新） |

### 命令协议（前端 → Rust，`invoke_handler` 全集）

`open_url(url)` · `quit_app()` · `get_settings()` · `save_settings(forumBase,source,pollIntervalSecs,theme)` ·
`report_ui_rects(rects)` · `get_latest_thread()` · `set_dragging(active)` · `move_window_by(dx,dy)` ·
`resize_float(width,height)` · `open_settings()`

## 核心机制（改这几处前必读）

### 1. 方案一' 穿透补偿（最重要）

Windows 透明窗口的"像素 alpha 自动穿透"**实测证伪**（spike 记录在 docs/superpowers/specs）。
现方案：**光标轮询 + 可见矩形上报**
- 前端每 250ms（及 hover/菜单等关键时刻）上报 `collectRects()`：`[ball, barwrap(+14 桥接), card(展开时), menu/cmenu(显示时)]`，物理像素 = CSS×dpr
- `run_cursor_watcher` 每 10ms 读光标与窗口位置，落在 rects 内 → `set_ignore_cursor_events(false)`，否则整窗穿透
- **拖动按下期**（`set_dragging`）watcher 冻结判定保持可交互
- ⚠️ **任何新的可见可交互元素都必须加进 `collectRects`**，否则鼠标一移上去就穿透（hover/点击全失效）——菜单就踩过这个坑
- ⚠️ rects 里出现**负坐标**（元素溢出窗口）会让判定失效，改布局后留意

### 2. 自绘拖动（为什么不用系统拖动）

`data-tauri-drag-region` 与 `window.start_dragging()`（HTCAPTION）都**实测有问题**：
- ACL 坑：`core:window:default` **不含** `allow-start-dragging`，drag-region 注入脚本无 `.catch()` → 拖动**静默失效**
- Snap 坑：HTCAPTION 拖动会触发 **Aero Snap**——拖到屏幕边缘把窗口压成别的尺寸、拖到最顶/最底"弹回"

现方案（ui/app.js）：`mousedown` 记录起点 → `mousemove` 位移 >4px 进入拖动 → 逐帧 `invoke('move_window_by',{dx,dy})`
（后端 clamp 到显示器但**允许贴边**）→ `mouseup/blur` 结束。`moved` 标志抑制拖动结束后的 click（避免误开网页）。

### 3. 动态窗口尺寸（窗口贴合内容）

- 基础：宽 = 球+条组合（**至少 380**，保证卡片 min-width 360 放得下）、高 = 球高量级 84
- 前端 `fitWindow(extraH)` 调 `resize_float`：卡片 hover 先撑高再显示、收起 280ms 后缩回；**右键菜单弹出也先撑高**（`br.bottom+6+mh+8`），关闭后缩回
- `resize_float` 后端会在扩展超出显示器右下缘时向左/上回缩，保证组合可见
- hud 贴窗口左上（`#hud{left:0;top:0}`），条 `margin-top:10px` 与球中心对齐；卡片 `position:absolute; top:66px; right:10px` **相对窗口**（不许再放回 barwrap，会跟随条宽溢出）

### 4. 提示音（Rust 侧播放）

`ui/assets/notify.mp3`（0.3s，提取自 ZCode `task-notification-pop`）用 `include_bytes!` 内嵌，
`poller.rs::play_notify()` 以 rodio+minimp3 独立线程播放——不走 WebView2（其 autoplay 需
`additionalBrowserArgs` 放开，而该参数会**污染之后创建的所有 WebView2 窗口**，见踩坑 #3）。
只在"运行期新帖"播放：`new-threads.initial=true`（首轮存量）与前端补拉不播。

## 透明窗镂空 + 流动光边（跨项目参考实现指南）

> 本节是 wqq-float 六轮实测沉淀的通用方案，其它 Tauri/WebView2 项目要做
> "异形镂空窗口 + 流动光边"直接照此路线，可跳过全部弯路。

### 需求与最终方案

需求：①桌面悬窗背景真镂空（白色桌面下看不到矩形窗体轮廓）②UI 圆角边缘平滑无锯齿
③鼠标交互完整（hover/拖动/点击）④整体轮廓有流动彩色光边。

**最终方案：`SetWindowRgn` 形状裁剪 + 前端形状外扩 2 物理像素 + 不透明 UI 背景。**

### 各路线实测裁定（勿走回头路）

| 路线 | 结果 | 弃用/采用原因 |
|---|---|---|
| 什么都不做 | ✗ | Win10 上 tao 为 `transparent:true` 窗口自动开 DWM blur-behind，透明区带灰雾，白桌面下整个矩形轮廓清晰可见（镂空失效的根源） |
| `SetWindowRgn` 直裁 | △ | 鼠标交互完整、无灰雾；但 region 是 **1bit 几何位掩码，天生无抗锯齿**（微软文档明确），边缘阶梯锯齿 |
| colorkey（`LWA_COLORKEY`） | ✗ | 视觉镂空最平滑（CSS 抗锯齿全保留），**但其像素级 hit-test 与 WebView2 的 mousemove 流冲突**：hover 全灭、拖不动（离散 click 反而能通，极具迷惑性）。另外穿透切换/启动时会丢 `WS_EX_LAYERED` 需反复补设。**需要完整鼠标交互的窗口一律别用** |
| accent blur / acrylic（SetWindowCompositionAttribute） | ✗ | 色雾更重 |
| per-pixel alpha（DirectComposition / `WS_EX_NOREDIRECTIONBITMAP`） | 未走 | 理论正解，但 wry 的 HWND 宿主不走此路，Tauri 未暴露 `no_redirection_bitmap`，自行集成 WebView2 COM 代价大 |

### region 方案的锯齿消除（核心技巧）

region 边缘是硬切，消除思路是**让锯齿像素藏在 UI 不透明区域内**：

1. 前端把每个 UI 形状的 `getBoundingClientRect()` **各方向外扩 2 物理像素**后上报
   （`app.js collectShapes()` → `set_window_region` 命令取并集 `SetWindowRgn`）。
   region 边缘比 CSS 形状大 2px，锯齿带落在 CSS 绘制范围内被覆盖，外露边缘走 CSS 抗锯齿。
2. 环带必须有内容：region 外扩后，CSS 形状边界之外、region 之内的一圈若没有任何
   绘制，WebView 透明区会显黑。用贴边光环（如 `#ball::before` inset:-2px 的 conic 边光）
   覆盖，且 **conic 全程保持非零 alpha**（最暗段 0.1+），否则暗段露出黑边。
3. 更彻底的遮盖：整体轮廓的流动光环（见下）本身就盖在 region 边缘上，锯齿不可见。

### 流动光边（圆形/圆角矩形通用跑马灯）

- 结构：独立 `#glow-ring` 层（absolute，`inset:-5px`，`border-radius` 取外轮廓圆角+5px），
  放在 UI 元素后面（z-index 最低），包裹整体外轮廓而非单个元素。
- **挖空方式用「内层伪元素盖底色」，不要用 `mask-composite: exclude`**——WebView2 上
  exclude 写法部分区域不渲染（右侧缺口）。实现：`#glow-ring` 整面 `conic-gradient`
  多彩分段 + `animation: rotate Ns linear infinite`；`::after` `inset:2.5px` 同圆角盖
  `--ring-cover` 纯色把中部挡住，只留 2.5px 环。
- `--ring-cover` 随主题取近底色（浅色主题近白、深色近黑），环内非 UI 空隙看起来仍近"空"。
- 注意：cover 层是实底色，被覆盖的环内空隙不再透出桌面——region 内的这些小空隙
  （如条上下的余量）用近底色视觉上无感，属可接受折中。

### 必须遵守的配套约束

1. **UI 背景一律不透明**：region + 透明 WebView2 下，**CSS 半透明背景像素整体丢失**
   （rgba 0.95 的卡片直接显示桌面，极难排查）。半透明质感用"不透明预混合色"模拟；
   `backdrop-filter` 对不透明背景无意义，一并移除。
2. **穿透切换会重置窗口样式**：`set_ignore_cursor_events` 经 tao `apply_diff` 重写
   `GWL_EXSTYLE`（切可交互时连 `WS_EX_LAYERED` 都会被移除）。region 不受影响
   （归系统持有），但 colorkey/LWA 类属性每次切换后都要补设。
3. **JS 上报函数里的变量必须先声明**：`collectShapes`/`collectRects` 里引用未定义变量
   会让整个上报函数抛异常 → `ui_rects` 停更 → 全窗穿透、所有交互静默失效
   （症状与 colorkey 冲突一模一样，先查 JS 控制台再怀疑 Win32）。
4. **诊断方法**：把窗口临时移到纯白桌面区域，对比窗口内外像素——很多"灰雾/黑块"
   其实是桌面壁纸和图标透过镂空正常显示，别误判。

### 涉及代码位置

- `ui/app.js`：`collectShapes()`（形状外扩上报）、`updateWindowRegion()`（sig 去重）
- `src-tauri/src/commands.rs`：`set_window_region`（GDI region 并集 + SetWindowRgn）
- `ui/index.html`：`#glow-ring` 层；`ui/app.css`：光环动画与不透明背景变量

## 踩坑速查（血泪台账，新坑请追加）

| # | 坑 | 症状 | 解法 |
|---|---|---|---|
| 1 | ACL 不含 `allow-start-dragging` | drag-region 拖动静默无效 | 改自绘拖动（机制 2） |
| 2 | **运行中创建 WebView2 窗口** | 设置窗首次能开、X 关闭后再开**整窗白屏且卡死** | ①setup **预创建**设置窗（`visible:false`+`skip_taskbar`）②拦截 `settings` 的 `CloseRequested` → `prevent_close`+`hide`（别让它销毁）③打开时 `show`+`eval("location.reload()")`（隐藏后不重绘） |
| 3 | `additionalBrowserArgs` | 设了它以后**所有后创建的 WebView2 窗口**渲染异常（设置窗白屏） | 别用（提示音已改 Rust 播放）；若必须用，注意它会替换 Tauri 默认 `--disable-features=...` |
| 4 | WebView2 IPC 回调里 `ExitProcess` | 球菜单"退出"无反应（托盘退出正常） | 独立线程延迟 80ms 退出（`quit_now`），先落盘配置 |
| 5 | HTCAPTION 拖动触发 Aero Snap | 拖到屏幕边缘窗口变形/拖不到顶底 | 自绘拖动（机制 2） |
| 6 | 自绘菜单未纳入 rects | 菜单显示但 hover 无高亮、点击无效（鼠标上去即穿透） | 菜单显示时 `collectRects` 纳入 + **立即** `reportRects()` |
| 7 | 卡片 `min-width:360` 相对窄条右对齐 | 空条时卡片向左溢出窗口被裁，rects 现负坐标 | 卡片改相对窗口定位 + 窗口宽 ≥380 |
| 8 | 首轮 poll 早于页面就绪 | 条永远空白（水位已推进只发 changed:0，事件丢失不可追） | `state.latest` + 前端 `get_latest_thread` 主动补拉 |
| 9 | `app_data_dir` 用 bundle identifier | 配置不在 `%APPDATA%\wqq-float\` 而在 **`%APPDATA%\cn.weiququ.float\`** | 排障/写脚本时用后者 |
| 10 | Config 加字段没 `serde(default)` | 旧配置整份作废（回落默认，用户设置全丢） | 新字段必须 `#[serde(default = "...")]` |
| 11 | `cargo run \| head -n` | `head` 截断管道 → SIGPIPE **杀死 cargo run** | 输出重定向到文件，别用 head 截断 |
| 12 | `document.title` 读窗口信息 | Tauri 窗口标题**不跟随** HTML title，读不到 | 需要前端→外部传值用后端日志/invoke |
| 13 | 透明窗残留矩形描边 | Win10 DWM 阴影在 layered 窗表现为描边 | `tauri.conf.json` 窗口 `"shadow": false` |
| 14 | `reqwest` 默认 native-tls | 部分网络环境下 schannel 吊销检查失败（`CRYPT_E_REVOCATION_OFFLINE`）连不上论坛 | 已用 `rustls-tls`（Cargo.toml），别改回默认 feature |
| 15 | 设置窗被遮挡时截图 | `ImageGrab` 截屏幕会看到下层窗口，误判"白屏" | `SetWindowPos(hwnd,-1,...)` 置顶截图后再 `-2` 恢复 |
| 16 | colorkey 的像素级 hit-test | hover 无反应、拖不动（click 却正常） | colorkey 与 WebView2 mousemove 冲突，交互窗改用 region 方案（见"透明窗镂空"一节） |
| 17 | CSS 半透明背景像素丢失 | region 镂空下卡片/条区域直接透出桌面 | region+透明 WebView2 丢弃半透明 alpha；UI 背景一律不透明预混合色 |
| 18 | 上报函数引用未定义变量 | 全窗穿透、交互静默全失效 | `collectShapes`/`collectRects` 抛异常 → rects 停更；先查 JS 控制台再怀疑 Win32 |
| 19 | `mask-composite: exclude` 挖空 | 流动光环部分区域不渲染（右侧缺口） | WebView2 兼容差；改「内层伪元素盖底色挖空」 |

## 开发流程

```bash
cd wqq-float/src-tauri
cargo test                 # 21 个单测（改 Rust 必须全绿）
cargo build                # ~30s
cargo run                  # 调试运行；后台启动时把输出重定向到文件（见坑 #11）
cargo build --release      # ~7min（LTO+strip），产物 target/release/wqq-float.exe
# 打包（DZ 拦裸 exe，必须 zip）
cd target/release && powershell -NoProfile -Command "Compress-Archive -Path wqq-float.exe -DestinationPath wqq-float-0.1.0-win64.zip -Force"
```

- **前端改动**：只重启进程（无需编译）
- **Rust 改动**：`cargo build` 前先杀进程（运行中会 `拒绝访问 os error 5`）
- **托盘图标/exe 图标**：仍是论坛站标（球体 UI 已换 favicon 素材 `favicon.jfif`→`logo.png`）

## 配置与数据

- 配置：`%APPDATA%\cn.weiququ.float\config.json`
  `{forum_base, source:"wqq"|"rss", poll_interval_secs(3~300), window_pos:[x,y], theme:"dark"|"system"}`
- 日志：同目录 `log.txt`（仅轮询失败时写入；零失败 = 文件不存在 = 正常）
- 轮询：默认 5s；连败 3 次退避 30s；wqq 连败 6 次自动切 RSS 兜底
- 接口契约（服务端 `DiscuzX5/wqq_notifier.php`）：
  `?last=<水位tid>` → `{ok:1, changed:0|1, top:<最大tid>, threads:[{tid,subject,author,dateline,fid,forumname,views,replies,digest,summary}]}`
  帖子 URL 客户端拼 `thread-{tid}-1-1.html`

## 排障技巧（自动化验证手段）

- **拿球/条精确坐标**：后端一次性日志打印前端上报的 `ui_rects`（`[[0,0,64,64],...]`，物理像素、相对窗口）—— 比截图目测/像素特征可靠得多
- **查穿透状态**：`GetWindowLongW(hwnd, GWL_EXSTYLE) & 0x20`（WS_EX_TRANSPARENT）——光标在球上时应为 0
- **模拟交互**：Python ctypes `SetCursorPos` + `mouse_event`（LEFTDOWN 0x0002 / LEFTUP 0x0004 / RIGHTDOWN 0x0008 / RIGHTUP 0x0010），窗口位置先枚举 GetWindowRect 拿实时值（用户会拖窗口）
- **看被遮挡的窗口**：`SetWindowPos(hwnd, -1 /*TOPMOST*/, ...)` 后截图，看完恢复 `-2`（NOTOPMOST）
- **找主窗口**：类名 `Tauri Window` + 标题 `Tauri App`（不是 productName）

## 未竟事项（接手候选）

1. 设置页「检查更新」仍指向论坛首页 —— 发布帖上线后改 `ui/settings.js` 的 URL
2. 提示音没有开关（设置页可加"静音"选项；音效版权上注意其来自 ZCode 资源）
3. NSIS 安装版暂缓（tauri-cli 装不上，见 .superpowers ledger Task 2）；便携单 exe 是主推产物
4. 托盘/exe 图标未换（球体已换）；高 DPI（125%/150%）与多显示器未实测（代码已按 dpr/clamp 处理）
5. Task 10 的独立 reviewer 一直未跑成，历轮以用户验收 + 自动化自测替代
6. 摘要里的 URL 是纯文本（安全考虑不自动转链接），已提供"打开原帖 ↗"兜底

## 历史与台账

- 开发台账（本地，不随 git 分发）：`.superpowers/sdd/2026-10-16-wqq-float/progress.md`（逐任务裁定与踩坑）
- 计划：`docs/superpowers/plans/2026-10-16-wqq-float.md`；设计与 spike：`docs/superpowers/specs/`
- 发布/验收记录（随 git）：`docs/release-checklist.md` —— **每轮用户反馈的根因都记在这里**
