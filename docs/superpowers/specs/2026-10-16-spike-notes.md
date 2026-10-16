# Spike 结论：透明窗穿透 / hover 命中 / drag（2026-10-16）

Task 3 决策门验证记录。方法：CUA a11y 树取元素屏幕坐标 + Win32 `WindowFromPoint` 命中测试（CUA/WGC 对透明 layered 窗口无法截图，改程序化验证）。

## 验证结果

**1. 穿透（原始假设）——证伪 ❌**

窗口矩形内四个测试点（长条中心、悬浮球、卡片收起区、纯透明空白区）**全部命中 WebView2 的子 HWND**（rect 与悬浮窗客户区 [60,52,760,572] 完全一致）。Windows 上 Tauri 2 / WebView2 的透明像素**不产生鼠标穿透**——完全透明区域照样拦截点击，会挡住下层应用。

**2. hover 展开机制——成立 ✓**

WebView2 内 DOM mousemove 照常派发，CSS `:hover` 按元素几何触发，展开/收起动画不受穿透问题影响。

**3. drag——留 Task 8 联动验证**

需在穿透补偿机制实现后验证（drag region 与单击共存）。

## 裁定：方案一'（光标矩形跟踪 + ignore_cursor_events 动态切换）

不转方案二（动态 set_size：动画期窗口抖动/撕裂，实现复杂度更高）。修补方案（Tauri 官方 transparent-windows 指南同思路）：

- Rust 侧 ~10ms 轮询 `GetCursorPos`（开销可忽略）
- 前端在可见 UI（球+长条+展开卡）变化时上报其屏幕矩形（`getBoundingClientRect` × DPI）
- 光标在矩形内 → `set_ignore_cursor_events(false)`（正常交互：hover/click/drag）
- 光标在矩形外 → `set_ignore_cursor_events(true)`（整个窗口穿透，事件落到下层应用）

交互完整性：所有用户交互只发生在光标入矩形时（ignore=false）；穿透生效于光标离开后（≤10ms 延迟，无感知）。mouseleave 触发切换与轮询闭环，无死锁路径。

## 代价与风险

- 新增 Rust 光标轮询循环与前端矩形上报协议（Task 8 范围）
- ignore 切换瞬间存在一帧边界竞态（轮询 10ms 内），最坏表现：穿透/交互生效晚 10ms
- 若 Task 8 实现中发现轮询方案异常，回退路径仍是方案二（动态 set_size）
