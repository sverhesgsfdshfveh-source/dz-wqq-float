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

## 流动光环 + 卡片改版（2026-10-17 第七轮）

| 项 | 实现 |
|---|---|
| 流动光环 | `#glow-ring` 包裹球+胶囊**整体外轮廓**（圆角矩形环 45px 圆角），conic 青蓝紫粉多彩渐变 4s 旋转；环体用「整面 conic + ::after 内层 2.5px 盖 `--ring-cover` 色挖空」实现——**mask-composite: exclude 在 WebView2 部分区域不渲染（右侧缺口），弃用** |
| 半透明像素丢失（关键坑） | region+透明 WebView2 下 **CSS 半透明背景像素整体丢失**（rgba(15,17,24,.95) 的卡片区域直接显示桌面）：alpha 通道在合成时被丢弃，只有不透明像素可见。条原本正常纯靠 backdrop-filter 产出的不透明层。修复：全部 UI 背景（条/卡片/菜单，含 hover 变量与浅色主题）改为**不透明预混合色**，并移除 backdrop-filter（不透明背景无磨砂意义） |
| 卡片布局 | top 66（与胶囊零间距）、左右缩进 30px（不超胶囊两端圆弧）、底部圆角 16、去顶边框；`CARD_TOP=66`、`openCard` 简化 |
| 详情卡 | 移除"浏览"显示（新帖浏览量小无意义），保留作者/版块/时间/回复 |
| JS 坑 | `collectShapes` 引用了未定义的 `hud` 变量 → `reportRects` 整个抛异常 → `ui_rects` 停更 → 全窗穿透、交互全失效（症状与 colorkey 的 mousemove 冲突一模一样，极具迷惑性）；已把 `hud` 加入顶部 const 声明 |

## 流动光边贴合整体轮廓（2026-10-18 第八轮）

| 现象 | 根因 | 修复 |
|---|---|---|
| **窗口中间出现"旋转的多彩形状"**（球/条上下的空隙里扫出彩块） | `#glow-ring` 用 `transform: rotate` 旋转整个元素：转的是矩形几何、不只是颜色分布——外接矩形环（宽 400+ × 高 90）斜过来时环带扫过条上下的 14px 空隙；而该空隙在 region（hud 外接矩形 +5px）内，故可见 | 光边改「`@property --ring-a` 角度动画」：几何静止、只滚 conic 起始角；删除外接矩形环 |
| 光边不贴 UI（悬在条上方 14px）、"底部 `#glow-ring` 层太宽" | 环按 hud 外接矩形画（圆角 45px），不是"球+胶囊"的真实轮廓 | 拆成两段：`#glow-ball`（85px 圆盘，被 80px 球盖住只剩 2.5px 边光）+ `#glow-bar`（胶囊外扩 2.5px 圆角矩形，`::after` 盖 `--ring-cover` 挖空） |
| （附带）条上下有近底色伪填充"假镂空" | region 用 hud 外接矩形时，空隙在 region 内却无 UI，只能靠 cover 层伪填充 | `collectShapes` 改报「球（kind 0 椭圆）∪ 胶囊（r26 圆角矩形）∪ 进度线（矩形）」，空隙真镂空、直接透出桌面 |
| （附带）进度线会被新 region 裁掉 | 进度线在胶囊下方 6px，不在"球∪胶囊"形状内 | 单独纳入 region（外扩 5px 容纳辉光）；`#progress` 加 `box-shadow` 辉光覆盖外扩带（顺带用上闲置的 `--prog-glow`） |
| （附带）球缘彩环发白 | `#ball::before` 的 2.5px 静态蓝白 conic 与彩环同区叠加，把流动色冲淡 | 移除；球缘光圈由 `#glow-ball` 独占（+球自身 1px inset 亮边） |

验证（WebView2 155.0.4283.60，浅色主题实测）：
- 同一取样点每隔 1.3s 取色持续变化 → `@property` 角度动画生效、光边在流动
- 窗口内 **0 个纯黑像素**（region 区域无露底/无黑边）；条上下直接透出桌面内容（真镂空）
- 球上 `WS_EX_TRANSPARENT=0`（穿透判定正常）；hover 胶囊 → 卡片展开、region 跟随、无裁切

## 锯齿回归修复 + 卡片背景跟随主题（2026-10-18 第九轮）

| 现象 | 根因 | 修复 |
|---|---|---|
| **球/胶囊整圈出现锯齿**（第八轮修完新暴露） | region 从"外接矩形（只有 4 个小圆角是弧）"改成"贴合球+胶囊轮廓（整圈圆弧）"，且第八轮光边是**高饱和硬边彩环**——1bit 裁边正好切在"亮彩环 vs 桌面"的硬跳变上，整圈弧全显锯齿 | 光边改**实心彩盘 + `filter: blur(2px)`** 外缘渐隐（不再用挖空层）；region 外扩 2→4px，让裁边落在模糊尾部 alpha≈0.05 处，两侧对比趋零；conic alpha 提到 0.6~0.98 补 blur 摊薄的亮度 |
| **展开卡片是黑色背景**（浅色主题下） | 第七轮"不透明预混合色"改造时 `#card` 背景写成硬编码深色渐变 `#191c26→#12141c`；`--card-bg: #fafbfe`（浅色）定义了却从未被引用——浅色主题下条/球/文字都是浅色，只有卡片是黑的 | 改 `--card-bg1/--card-bg2` 变量驱动（dark: #191c26→#12141c；light: #ffffff→#f3f6fc） |

验证（WebView2 155，浅色主题，黑/白两种桌面背景）：
- 球缘径向扫描（黑底）：窗口 x=2（region 边界）处 #191322（≈4% 亮度），向外平滑渐变 4px 无台阶；5x 放大目检无锯齿
- 浅色桌面下 idle / hover 窗口内**纯黑像素均为 0**（region 无露底），进度线辉光带过渡正常
- 光边流动采样（1.3s 间隔持续变化）：球左缘 89e4ff→ebccfe→c1b9ff
- 卡片背景实测 #fcfdfe/#f7f9fd（浅色主题正确）、文字深色可读

## 设置窗体验（2026-10-18 第十轮）

| 项 | 实现 |
|---|---|
| 高度自适应 | 设置窗固定 420x410 装不下内容（WebView2 实测内容高 574）出现滚动条；headless Edge 测量 520 与真实渲染差 54px，靠估算不可靠 → 新增 `resize_settings(height)` 命令；`settings.js` 加载后按 `document.body.scrollHeight + 2`（逻辑像素）上报自适应。**用 body 而非 documentElement**：内容比视口矮时后者的 scrollHeight 返回视口高度，永远测不出"缩小"。实测收敛到 436x615（内容区 576），无滚动条；以后改 settings.html/css 不用再动窗口尺寸常量 |
| 保存后自动收起 | 点「保存」→ 保存成功并提示 → 400ms 后 `hide_settings()` 收起窗口（hide 保活，与 X 按钮同机制；销毁后重建会白屏）。保存失败不收起，保留错误提示 |
| 初值兜底 | main.rs / commands.rs 两处 inner_size 初值 420x620（页面加载后立即被自适应覆盖） |

## 常量备忘

- 轮询默认 5 秒（3~300 可配）；连败 3 次退避 30 秒；wqq 连败 6 次自动切 RSS
- 配置：`%APPDATA%\cn.weiququ.float\config.json`；日志：同目录 `log.txt`（仅失败时写入）
  （Tauri `app_data_dir` 用 bundle identifier 命名，非 productName）
- 数据接口：`wqq_notifier.php`（已在 DiscuzX5 README 二开保护清单登记）
- 悬浮窗：球 80px（压住胶囊左端 40px，胶囊左半圆完全藏于球后）+ 条高 52px/max 640px，窗口高 100（卡片展开时动态加高）；
  拖到屏幕右缘 40px 内自动吸附（右边距 8px）
- 流动光边（第八/九轮）：球环 85px 实心圆盘（r42.5，露出 2.5px）+ 胶囊环 = 实心圆角矩形
  胶囊 +2.5px（`left:37.5px; top:11.5px; right:-2.5px; height:57px; r28.5`）；
  `@property --ring-a` 4s 线性流转；`filter: blur(2px)` 外缘渐隐；conic alpha 0.6~0.98。
  region 形状 = 球（椭圆）∪ 胶囊（r26）∪ 进度线，**外扩统一 4px**（进度线 5px）——
  外扩带必须落在光边 blur 渐隐尾部，否则弧边锯齿现形
- **colorkey 约束：UI 内禁止出现纯黑 RGB(0,0,0) 像素**（当前主题最深 ~RGB(11,13,19)，安全）；改主题色时注意别引入 #000
