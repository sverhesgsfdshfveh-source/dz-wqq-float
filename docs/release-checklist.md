# wqq-float 0.1.0 发布清单

## 产物

- **便携单 exe（主推）**：`src-tauri/target/release/wqq-float.exe`
  - 图标为论坛自定义站标（md5 d503670d），前端资源已内嵌
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

## 常量备忘

- 轮询默认 5 秒（3~300 可配）；连败 3 次退避 30 秒；wqq 连败 6 次自动切 RSS
- 配置：`%APPDATA%\wqq-float\config.json`；日志：同目录 `log.txt`（仅失败时写入）
- 数据接口：`wqq_notifier.php`（已在 DiscuzX5 README 二开保护清单登记）
