# wqq-float 0.1.0 发布清单

## 产物

- **便携单 exe（主推）**：`src-tauri/target/release/wqq-float.exe`
  - 图标为论坛自定义站标（md5 d503670d），前端资源已内嵌
  - 实测体积 11.8MB（zip 后 3.9MB；tauri 全家桶 + rustls + 内嵌前端，strip+lto 已启用）
  - 目标体积 ~3-5MB，双击即用，无安装步骤
- **NSIS 安装版（可选，暂缓）**：tauri-cli 因 zip crate 全线 yank 无法安装（见 ledger Task 2 裁定），NSIS 打包暂缺；便携 exe 已覆盖全部功能（自启走注册表 HKCU\Software\Microsoft\Windows\CurrentVersion\Run，无需安装器）

## 用户引导文案（发布帖用）

> 首次运行若弹出 Windows SmartScreen 提示「已保护你的电脑」，点击
> **「更多信息」→「仍要运行」** 即可（程序暂未购买代码签名证书，属正常提示）。
> 若双击无反应，请安装微软 WebView2 Runtime：
> https://developer.microsoft.com/microsoft-edge/webview2/

## 附件打包

```bash
cd src-tauri/target/release && zip wqq-float-0.1.0-win64.zip wqq-float.exe
```

- 论坛附件名建议 `wqq-float-0.1.0-win64.zip`（DZ 默认拦裸 exe 附件，必须打 zip）
- 发布帖标题建议：`桌面悬浮线报通知工具 wqq-float v0.1.0（官方版）`
- 「检查更新」按钮当前指向论坛首页，发布帖发好后把 `ui/settings.js` 里
  `check-update` 的 URL 改为帖子地址再出下一版

## 验收记录（2026-10-16）

| 项 | 结果 |
|---|---|
| Rust 单测（JSON/RSS/配置/退避/水位/兜底） | 19/19 通过 |
| 服务端 wqq_notifier.php 三态（curl 实测） | 通过 |
| 启动 → 长条渲染最新帖（a11y 证据） | 通过 |
| 真实新帖到来滚动切换（京东发小红薯→第二轮李佳琪） | 通过 |
| 相对时间/版块/作者/回复/浏览渲染 | 通过 |
| 默认位置=主屏右缘垂直居中 | 通过（1196+716=1912，1920-8） |
| 轮询日志无错误（log.txt 未生成=零失败） | 通过 |
| **待人工终验** | hover 展开动画手感、拖动+位置记忆、球右键菜单、点击打开帖子、托盘四项菜单、开机自启勾选、透明区点击穿透（方案一'） |

## 验收记录（2026-10-17 用户反馈轮）

| 项 | 结果 |
|---|---|
| Rust 单测 | 20/20 通过（新增拖动保持/超时解冻） |
| UI 对标样板图（球换 favicon 红鹤 logo、64px 发光环、红点徽标、胶囊条、进度线、球条间隙 8px） | 通过（截图确认） |
| 拖动 | 通过（用户实测 OK；根因修复见下） |
| 透明窗残留边框 | 通过（`shadow: false`） |
| 位置存档越界回落（-22 顶部越界 → 默认右缘居中） | 通过（实测） |
| 右缘吸附（窗口移到 gap=30px 处 → 自动贴 gap=8px） | 通过（SetWindowPos 实测） |
| 首轮数据竞态（poll 早于页面就绪 → 条空白） | 通过（前端补拉后截图确认真实帖渲染） |
| hover 展开卡片且条不位移 | 通过（截图确认） |

### 本轮修复的根因（供排障）

- **拖动失效**：Tauri v2 的 `core:window:default` 权限集**不含** `allow-start-dragging`，
  `data-tauri-drag-region` 注入脚本在 mousedown 时 invoke `plugin:window|start_dragging`
  被 ACL 拒绝且脚本无 `.catch()` → 静默失败。已改为前端阈值拖动（位移 4px 触发）+
  自定义 `start_drag` 命令（Rust 侧调用不受 ACL 限制）+ `set_dragging` 在拖动期冻结穿透判定。
- **首轮条空白**：首轮 poll 常早于 WebView 页面就绪，`new-threads` emit 丢失；
  之后水位已推进只发 `changed:0`，条永远空白。已加 `state.latest` + `get_latest_thread`
  命令，前端加载完成后主动补拉。

## 新帖提示音（2026-10-17 追加）

- 音效文件 `ui/assets/notify.mp3`（0.3 秒），提取自 ZCode 的 `task-notification-pop`
- 行为：**运行期间**有新帖到达 → 播放；启动首帧、补拉、首轮 20 条存量帖**不播**
  （poller 在 `new-threads` 事件里带 `initial` 标记，首轮水位为 0 时置 true）
- **实现走 Rust 侧**（rodio + minimp3 解码内嵌 mp3，独立线程，播放失败静默）：
  最初试过前端 `new Audio` + WebView2 的 `--autoplay-policy=no-user-gesture-required`，
  但实测该 `additionalBrowserArgs` 会波及后创建的 WebView2 窗口——**设置窗整窗白屏**，
  故放弃该参数，提示音移到 Rust 播放，前端不再持有音频对象

## 2026-10-17 第二轮用户反馈修复（供排障）

| 现象 | 根因 | 修复 |
|---|---|---|
| 球与条"在窗体中变换位置"、卡片被裁 | ①flex 顺序为球左条右，条宽随标题变化把球推着走；②拖动用 HTCAPTION 触发 Windows Aero Snap，实测窗口被压成 760x412/443，卡片展开即被窗口底裁掉 | ①`flex-direction: row-reverse`：球固定贴窗口右缘做锚点，条向左延伸；②`settle_geometry`：移动停止 400ms 后恢复基准尺寸（setup 时记录）+ 位置 clamp + 右缘吸附 |
| 右键菜单跑到奇怪位置、hover 无反馈、点击无效 | 菜单区域未纳入穿透判定 `ui_rects`：鼠标移到菜单上即被判"窗外"→ 窗口穿透 → hover/click 全部失效；定位用右键点坐标，随点击位置漂移 | 菜单显示时纳入 rects 并即时上报；定位改贴球下方居中（下方空间不足自动翻到球上方） |
| 设置窗打开全白 | **三个递进根因**：①float 窗口的 `additionalBrowserArgs` 影响后续创建的 WebView2 窗口；②**运行中首次创建**设置窗本身白屏（启动时创建正常）；③点 X 时 Tauri 默认**销毁**窗口，第二次打开又变成"运行中重新创建"→ 白屏且窗口卡死（X 无响应） | ①移除该参数（提示音改 Rust 播放）；②setup **预创建**设置窗（`visible:false` + `skip_taskbar`）；③拦截 settings 的 `CloseRequested`：`prevent_close` + `hide`（窗口保活），打开时 `show` + `eval("location.reload()")` 解决隐藏后不重绘 |
| 球菜单"退出"没反应（托盘退出正常） | 球菜单走 WebView2 IPC 回调，在回调线程内直接 `ExitProcess` 会挂起；托盘菜单回调无此问题 | `quit_now` 改独立线程延迟 80ms 退出（先落盘配置） |

## 交互与外观（2026-10-17 第三轮追加）

| 项 | 实现 |
|---|---|
| 主题设置 | 设置页「纯黑（默认）/ 跟随系统」；`config.theme`（serde default 兼容旧配置）；CSS 变量化 + `prefers-color-scheme: light`，球体保持深色；保存后 `settings-changed` 事件让悬浮窗即时刷新 |
| 文字可选中复制 | 卡片 `user-select: text`（摘要可框选 + Ctrl+C）；条与球保持不可选，避免干扰拖动/点击 |
| 内容右键菜单 | 卡片右键 → 自绘「复制 / 全选」；全局 `contextmenu` preventDefault 兜底屏蔽 WebView2 默认菜单 |
| 打开原帖 | 卡片底部「打开原帖 ↗」链接 |
| 菜单收起 | 菜单开着时点球/条**只收菜单**（不再顺手打开网站/帖子）；点其他应用或穿透空白区（窗口失焦）自动收起；Esc 也可关闭。失焦由后端 `WindowEvent::Focused(false)` → `float-blur` 事件驱动（穿透区域前端收不到点击，只能靠焦点事件） |

## 窗口与拖动（2026-10-17 第四轮：贴合内容 + 自绘拖动）

| 项 | 实现 |
|---|---|
| 窗口动态尺寸 | 不再固定大窗：宽=球+条组合宽（至少容纳卡片 360px），高=球高量级（84px）。前端 `resize_float` 命令驱动：卡片 hover 展开先撑高（+卡片高）再显示、收起 280ms 后缩回；右键菜单弹出时也先撑到能容下菜单（球菜单实测 84→192），关闭后缩回 |
| 卡片布局 | 卡片移出 `barwrap`、改为相对窗口右对齐定位（`top:66px; right:10px`），修复空条时 min-width 360 向左溢出窗口被裁、以及 ui_rects 出现负坐标的问题；hover 改 JS 管理（条与卡片间 120ms 宽限） |
| 自绘拖动 | **不再用系统 HTCAPTION 拖动**（会触发 Aero Snap：拖到屏幕边缘把窗口压成别的尺寸/弹回）→ 前端 mousemove 增量 + `move_window_by` 命令，clamp 到显示器但**允许贴边**。实测可贴屏幕最顶（y=0）与最底（底=1080）；拖动后的 click 由 moved 标志抑制 |
| 球体随主题 | 球体渐变/光环/徽标描边变量化，浅色主题下球体同步变浅色（此前只有条和卡片变色，球仍是深色不和谐） |
| 移除 | `start_drag`（系统拖动）与 `base_size` 尺寸守卫（尺寸改由前端管理）；hud 贴窗口左上、条下移 10px 与球中心对齐 |

## 通知条视觉重设计（2026-10-17 第五轮：球条无缝融合）

| 项 | 实现 |
|---|---|
| 球条融合 | **球 64→80px，条 44→52px**；条左移 40px（球半径，`#ball{margin-right:-40px}` + 条 `z-index` 低于球），胶囊左半圆（半径 26px，最远点距球心 √(26²+26²)≈36.8px）完整藏进球内 → 连接处看不到任何胶囊转角圆弧；条 `padding-left:74px`，文字距球右缘约 34px |
| 球体 | 径向渐变深蓝黑底 + `conic-gradient` 渐变光环（`mask` 裁成 2.5px 边）+ 顶部高光 + 双层蓝色辉光 + 底部投影；**移除未读红点**（设计稿红点是 logo 图案的一部分，非功能徽标） |
| 胶囊 | 深色/浅色磨砂玻璃（`backdrop-filter: blur(20px) saturate(1.4)`）+ 1px 边框 + 顶部高光 + hover 提亮；标题 15.5px/500，时间前置细竖线，右侧时间用等宽数字 |
| 进度线 | 条底 +6px（`top:58px`），左右各内缩 14px，白/蓝灰渐变 + 微光 |
| 布局联动 | **卡片 top 66→88px**（球底 +8）、`CARD_TOP` 同步；悬停桥接带 14→24px（`#barwrap::after` 与 `collectRects` 扩展同时改）；窗口初始高 84→100 |
| 注意 | 实测 `cargo build` 的 debug exe **同样嵌入 ui/ 资源**，改前端后必须重新编译再重启（原"改前端不用编译"记录作废） |

## 透明窗"镂空"（2026-10-17 第六轮：region 裁剪 + 1px 外扩抵消锯齿）

| 项 | 实现 |
|---|---|
| 问题 | Win10 上 tao 为透明窗口启用 DWM blur-behind，透明区域带一层灰雾——白色桌面下整个矩形窗体轮廓清晰可见，"镂空"完全失效 |
| 三条路线实测裁定 | ①colorkey（LWA_COLORKEY）：视觉镂空完美平滑，**但其像素级 hit-test 与 WebView2 的 mousemove 流冲突——hover 无反应、拖不动、卡片失效**（离散 click 反而能通，极具迷惑性），废弃；②accent blur/acrylic：色雾更重，废弃；③**region（SetWindowRgn）**：鼠标交互完整、无灰雾，唯一缺点是 1bit 裁剪边缘有锯齿 |
| 最终方案 | **region 裁剪 + 每形状外扩 1 物理像素**：前端 `collectShapes()`（app.js）把球椭圆/胶囊圆角/进度线/卡片/菜单的 getBoundingClientRect 各方向 +1px 上报，Rust `set_window_region` 命令取并集 SetWindowRgn。外扩让锯齿像素落在 CSS 形状自身边界内被覆盖，外露边缘走 CSS 抗锯齿。`reportRects`（250ms 周期 + hover/菜单等关键时机）顺带更新 region，窗口尺寸变化自动跟随 |
| 穿透判定不变 | `ui_rects`（精确矩形）继续管鼠标穿透；region 只管视觉裁剪，两者独立 |
| CSS 约束 | 外投影/外辉光会被 region 裁掉 → 立体感全用 inset 阴影 + 1px 边框实现（`#ball.warn` 也是 inset） |
| 排障备忘 | region 是归系统所有的句柄，`SetWindowRgn` 成功后不能再 DeleteObject；窗口 resize 后前端自动重报新形状 |

## 常量备忘

- 轮询默认 5 秒（3~300 可配）；连败 3 次退避 30 秒；wqq 连败 6 次自动切 RSS
- 配置：`%APPDATA%\cn.weiququ.float\config.json`；日志：同目录 `log.txt`（仅失败时写入）
  （Tauri `app_data_dir` 用 bundle identifier 命名，非 productName）
- 数据接口：`wqq_notifier.php`（已在 DiscuzX5 README 二开保护清单登记）
- 悬浮窗：球 80px（压住胶囊左端 40px，胶囊左半圆完全藏于球后）+ 条高 52px/max 640px，窗口高 100（卡片展开时动态加高）；
  拖到屏幕右缘 40px 内自动吸附（右边距 8px）
- **colorkey 约束：UI 内禁止出现纯黑 RGB(0,0,0) 像素**（当前主题最深 ~RGB(11,13,19)，安全）；改主题色时注意别引入 #000
