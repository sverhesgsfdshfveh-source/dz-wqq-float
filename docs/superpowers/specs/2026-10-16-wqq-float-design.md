# 微趣趣桌面悬浮通知工具（wqq-float）设计文档

- 日期：2026-10-16
- 状态：设计已确认（待实施）
- 关联：论坛生产站点 bbs.weiququ.cn（Discuz! X5.0），本地基线仓库 `DiscuzX5/`

## 一、背景与目标

微趣趣线报网的线报帖时效性极强（红包/活动线报以分钟计价）。站方已有首页线报流 5 秒轮询刷新（`wqq_live.php`）和旧浏览器扩展（MV2，无法更新，3 秒轮询 `sitemap.php`），但用户在桌面干别的事时看不到新帖。本项目做一个 Windows 桌面常驻「悬浮球 + 长条」客户端：

- 屏幕右缘垂直居中显示悬浮球（论坛 favicon）+ 横向长条（最新一条帖子的标题，宽度随标题长度动态变化）
- 鼠标移到长条上向下缓动展开帖子详情卡
- 点击长条打开网页版帖子，点击球打开论坛首页
- 新帖到来时长条内容向下滚动切换为最新帖
- 开机自启、单 exe、可分发给论坛用户下载
- 架构预留多论坛扩展（数据源抽象层）

## 二、已确认决策

| 维度 | 决定 |
|---|---|
| 目标用户 | 分发给论坛用户下载（预配本站的官方版） |
| 实时性 | 秒级：本站走专用 JSON 接口；其它 DZ 论坛降级 RSS（分钟级） |
| 多论坛 | 本期单论坛；`ThreadSource` 抽象 + 配置模型按多源预留 |
| 通知方式 | 仅长条滚动动画，不弹系统 toast |
| 技术栈 | Tauri 2 + Rust + 纯静态 HTML/CSS/JS（无 Node 构建链），单 exe |
| 窗口架构 | 方案一：恒定尺寸透明窗口 + Windows 像素穿透 + 全 CSS 动效 |
| 服务端改动 | 新增 `wqq_notifier.php`（只读 JSON 接口，进 DiscuzX5 仓库） |

## 三、总体架构

```
工作区/
├── DiscuzX5/                  # 现有论坛 git 仓库（唯一事实源）
│   └── wqq_notifier.php       # 新增：桌面客户端专用只读 JSON 接口
└── wqq-float/                 # 新建：Tauri 2 项目（独立 git 仓库）
    ├── src-tauri/             # Rust：轮询/源适配/配置/托盘/自启/窗口
    ├── ui/                    # 静态 HTML+CSS+JS（frontendDist 直接指向，零构建链）
    └── docs/                  # 本设计文档等
```

运行形态：常驻单进程（`wqq-float.exe`），两个窗口——悬浮主窗（透明、无边框、置顶、跳过任务栏）与设置窗（普通小窗，按需创建）。系统托盘常驻。Rust 侧负责所有网络轮询（webview fetch 远程域名会撞 CORS，这是硬约束），前端只渲染。

## 四、服务端接口 `wqq_notifier.php`

### 契约

```
GET wqq_notifier.php?last=<客户端水位tid>

无新帖: {"ok":1,"changed":0,"top":334535}
有新帖: {"ok":1,"changed":1,"top":<新最大tid>,"threads":[
  {"tid":334536,"subject":"标题","author":"作者","authorid":1,
   "dateline":1760000000,"fid":2,"forumname":"综合线报",
   "views":12,"replies":0,"digest":0,"summary":"首帖前250字…"}]}
```

- threads 按 dateline 倒序，最多 20 条；`top` 为本列表最大 tid（客户端新水位）
- 帖子 URL 客户端自行拼伪静态 `thread-{tid}-1-1.html`，接口不返回

### 查询设计（照抄已验证的 wqq_live.php 模式）

- 检测：`SELECT tid FROM pre_forum_thread WHERE tid > last AND displayorder >= 0 ORDER BY tid DESC LIMIT 1`——主键范围倒序，命中 1 行即返，常量级
- 出列表：读最近 1000 条候选（`tid > top-1000` 主键倒序），PHP 内过滤 `displayorder>=0`，按 dateline 倒序（同秒按 tid 倒序保证稳定）取 20 条
- 版块名：forum 表全量（几十行）一次取完做映射
- 依据：`pre_forum_thread` 无单列 dateline 索引，`ORDER BY dateline` 全表 filesort 0.25s/次会打满 MySQL（README 踩坑清单第 20 条实测）；tid 与发帖时间同向

### summary 规则

- 复用 DZ 自带 `libfile('function/post')` 的 `threadmessagecutstr()`（RSS 同款）：首帖内容前 250 字
- 阅读权限帖（readperm>0）、出售帖（price>0）、被屏蔽帖给空串——与 RSS 行为一致，不泄露受限内容

### 安全与部署

- 只读不写库；游客视角（`groupid=7`、`uid=0`），仅 `displayorder>=0` 公开帖；`last` 强制 intval；SQL 走 `%t/%d` 占位符
- 响应头：`application/json`、`no-store`、`X-Content-Type-Options: nosniff`
- `init_cron=false`、`cachelist` 最小化（同 wqq_live.php）
- 部署：本地改 → scp 上服务器 → curl 验证 → DiscuzX5 仓库 commit；**登记进 README 第九节二开保护清单**

## 五、客户端架构（Rust）

### 模块划分

```
src-tauri/src/
├── main.rs        # 插件注册（opener/autostart/single-instance/tray）、窗口创建
├── source.rs      # trait ThreadSource → WqqApiSource + RssSource
├── poller.rs      # tokio 轮询循环 + 退避
├── config.rs      # %APPDATA%\wqq-float\config.json 读写
├── state.rs       # 水位 tid、当前展示帖
└── commands.rs    # Tauri command：打开URL、上报窗口位置、保存设置
```

### 数据源抽象

```rust
enum PollResult { Unchanged { top: u64 }, Changed { top: u64, threads: Vec<Thread> } }
trait ThreadSource { async fn poll(&self, last_tid: u64) -> anyhow::Result<PollResult>; }
```

- `WqqApiSource`：reqwest 请求 `wqq_notifier.php?last=N`，解析 JSON——本站秒级通道
- `RssSource`：reqwest 请求配置论坛地址下的 `forum.php?mod=rss`，quick-xml 解析 RSS 2.0 映射到 Thread 模型（link 直接用 item 自带 URL，不拼伪静态）——任意 DZ 论坛通用通道
- 配置项 `source: "wqq" | "rss"`（默认 `wqq` + 预配本站）；换论坛地址的用户在设置页选 `rss`
- 多论坛扩展路径：未来每论坛一个 source 实例 + 聚合器，Thread 模型加 origin 字段，UI 不动

### 轮询与退避

- 默认 5 秒间隔（可配 3~300 秒）；连续失败 3 次退避到 30 秒，成功即恢复 5 秒
- 休眠恢复由 tokio interval 天然继续，水位机制保证不漏帖
- WqqApiSource 持续失败时自动切 RssSource 兜底，设置页提示当前通道

### 事件与命令

- Rust `emit("new-threads", threads)` → 前端滚动动画；`emit("connection-state", state)` → 球警示圈
- 前端调 Rust command：`open_url(url)`（tauri-plugin-opener，默认浏览器）、`save_window_position(x,y)`、`save_settings(...)`

### 插件清单

tauri-plugin-opener（开链接）、tauri-plugin-autostart（注册表 Run 键）、tauri-plugin-single-instance（重复启动聚焦已有实例）、tray（托盘 + 右键菜单：打开论坛/设置/开机自启勾选/退出）

## 六、UI 交互规格（前端 ui/）

### 主窗口

- 恒定 ~700×520，`transparent` + `decorations:false` + `always_on_top` + `skip_taskbar`
- 默认位置：主屏右缘垂直居中；不可见区域 alpha=0——Windows layered window 完全透明像素自动鼠标穿透，不挡下层应用；命中测试按像素 alpha，`:hover` 仅在可见长条上生效
- DPI/分辨率变化后启动时 clamp 回可视区

### 悬浮球

- 48px 圆，论坛 favicon 图标（本地资源内嵌，取 `DiscuzX5/favicon.ico` md5 d503670d 自定义版）
- 左键单击：打开论坛首页
- 右键：自绘小菜单（打开论坛/设置/退出）
- 拖动柄：球区域 `data-tauri-drag-region`，拖动整窗；松手上报位置存配置

### 长条

- 深色半透明（rgba(20,20,20,0.85)）+ 毛玻璃（`backdrop-filter: blur`），圆角
- 内容 `fit-content`，宽度随标题动态变化；超 ~600px 省略号截断
- 右端相对时间「N分钟前」，前端每 30 秒刷新
- 点击长条整体：打开该帖 `thread-{tid}-1-1.html`
- hover：详情卡自长条下方缓动展开（`max-height` + `cubic-bezier` 缓出 ~250ms）：标题、首帖摘要、作者、版块、时间、回复/浏览数；移出收起

### 新帖滚动

- `new-threads` 事件 → 旧标题向下滑出、新标题自顶部滑入（CSS transition translateY）
- 多条同时到达直接展示最新一条（本期不做展开卡内历史列表）

### 连接状态

- 轮询失败：球加警示色圈 + 长条时间区显示「重连中」，静默退避，不弹任何打扰

### 设置页（普通小窗 ~420×360，托盘或球右键打开）

- 论坛地址（预填 `bbs.weiququ.cn`）、源类型（微趣趣增强 / 普通 DZ 论坛 RSS）
- 轮询间隔（3~300 秒，默认 5）
- 开机自启开关
- 「检查更新」= 打开发布帖链接
- 版本号；首启建议开自启的提示

## 七、数据流全景

```
启动 → 读配置（无则默认：本站/wqq/5s/不自启）→ 恢复窗口位置
     → 首次轮询(last=0 取全量) → 长条显示最新帖
稳态 → 每5秒 GET wqq_notifier.php?last=水位 → 多数 changed:0（1行查询）
新帖 → changed:1 + threads → 更新水位 → emit → 长条滚动动画
点击 → open_url → 默认浏览器打开帖子/首页
```

## 八、错误处理矩阵

| 故障 | 行为 |
|---|---|
| 网络断 / 服务器 5xx | 静默退避（连败 3 次 → 30 秒），球警示圈 +「重连中」，恢复回 5 秒 |
| wqq_notifier.php 异常 | WqqApiSource 连败自动切 RssSource 兜底，设置页提示 |
| JSON / RSS 解析失败 | 同退避 + 写 `%APPDATA%\wqq-float\log.txt`（用户反馈排障用） |
| 配置文件损坏 | 重建默认配置，不崩 |
| 分辨率/显示器变化 | 下次启动 clamp 回可视区 |
| 系统休眠 | 恢复后下一轮补齐（水位不漏帖） |

## 九、测试策略

- Rust 单测：JSON 契约解析、RSS XML 解析（fixture 用真实响应样本）、水位/changed 判定、退避状态机、配置读写、损坏配置恢复
- 服务端：`php -l`；curl 三态（last=0 / changed:0 / changed:1）；displayorder 过滤；readperm 帖 summary 为空
- **Spike（实施第一个任务）**：Tauri 透明窗 + 像素穿透 + hover 命中 + drag-region 与单击冲突，失败即升级方案二（Rust 动态 set_size）
- 手工验收：动画流畅性、透明区点击落到下层窗口、拖动 + 位置记忆、自启、多显示器 DPI、常驻 8 小时内存曲线（目标 <120MB）

## 十、分发与版本

- 产物：便携单 exe（~3-5MB，主推）+ NSIS 安装版（可选）
- 无代码签名：首次运行 SmartScreen 提示，发布帖截图引导「更多信息→仍要运行」；将来可买 OV 证书根治
- 渠道：论坛发布帖，exe 打 zip 做附件（DZ 拦裸 exe）+ 网盘外链备用
- 版本 0.1.0 起；「检查更新」打开发布帖（本期不做应用内自动更新）

## 十一、非目标（本期不做）

- Windows 系统 toast 通知
- 多论坛同时监控的聚合 UI（仅预留数据层）
- 应用内自动更新 / 代码签名
- 展开卡内新帖历史列表、已读管理
- macOS / Linux 版本

## 十二、风险与对策

| 风险 | 对策 |
|---|---|
| Windows 透明窗 hover 命中行为不符预期 | Spike 前置验证；失败转方案二（动态 set_size，复杂度 +1 档） |
| 目标机器无 WebView2 Runtime | 随 Edge 分发覆盖率接近全量；发布帖写兜底安装指引 |
| 服务器升级覆盖 `wqq_notifier.php` | 登记二开保护清单（同 wqq_live.php 待遇）+ 客户端 RSS 自动兜底 |
| 论坛附件拦截 exe | zip 打包 + 网盘外链 |
| drag-region 与单击冲突 | Spike 一并验证；不行改球内小拖动区或右键菜单移动 |
