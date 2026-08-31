# LGUI 目录重构工作记录

> 临时文档。用于记录本轮目录重构的目标、实际落地内容、环境准备和验收结果。
> 重构稳定合并后，可删除本文档；长期有效的结构说明已经同步到 `ARCHITECTURE.md`。

## 状态

- 总体状态：已完成
- 基线提交：`0e8b81c`
- 完成日期：2026-08-31
- 重构单位：保留单个 `lgui` crate，不拆分为多个 crate
- 兼容约束：保留公共 Rust 路径、Cargo Feature 名称和运行时语义

## 目标

1. 让物理目录直接表达 application、command、events、router、store、diagnostics 等子系统。
2. 将 Core 按 foundation、view、component、input、layout、scene 分组。
3. 将 Session、Host、Frame 统一归入 runtime。
4. 分离渲染契约、Skia 实现、Winit 适配器和 Win32 平台实现。
5. 将 assets、text、theme、services 和复杂 widgets 从单文件拆成职责明确的目录。
6. `mod.rs` 仅保留模块声明、条件编译和导出。
7. 不引入业务页面、业务 Store、网络协议实现或第二套 GUI Runtime。

## 最终顶层结构

```text
src/
├─ application/
├─ command/
├─ events/
├─ router/
├─ store/
├─ core/
├─ runtime/
├─ renderer/
├─ platform/
├─ services/
├─ assets/
├─ text/
├─ theme/
├─ widgets/
└─ diagnostics/
```

## 已完成内容

### Application、Command、Events、Diagnostics

- `application.rs` 已拆为 builder、backend、context、handle、error、view、renderer selection、
  notification、tray 以及 `window/`。
- Command 已拆为 contract、context、registry、handle。
- Events 已拆为 contract、bus、subscription。
- Diagnostics 已拆为 model、collector、provider、system、timing。
- 三个子系统的单元测试均从 `mod.rs` 移入独立 `tests.rs`。

### Router 与 Store

- Router 分成 `matcher/`、`runtime/`、`declarative/`。
- Runtime 内进一步分离 history、snapshot、subscription 和 router。
- Declarative Router 分离 route、builder、outlet、redirect。
- Store 分成 definition、unit、hooks、subscription 和 `runtime/` registry/engine。

### Core 与 Runtime

- Core 文件按 foundation、view、component、input、layout、scene 物理归档，同时保留
  `lgui::core::*` 兼容导出。
- Component Runtime 分为 state、input、action、animation、focus。
- Scene Render 分为 primitive、scene、compiler、transform、phase、damage。
- Session、Host、Frame 的实现统一归入 `runtime/`。
- Host 分为 model、storage、reconcile、commit、scene、semantics、damage。

### Renderer、Assets、Text、Theme、Widgets

- 渲染契约和缓存归入 `renderer/`，平台无关 Skia 后端归入 `renderer/skia/`。
- Skia 后端进一步分为 support、text、cache、software surface、painter、primitives。
- Assets 分为 model、resolver、cache、resources、custom paint、icons。
- Text 分为 model、fonts、layout、service。
- Select 分为 model、state、render；Slider 分为 model、state、math、render。

### Winit 与 Win32

- Winit 分为 application、event loop、window、input、renderer、accessibility 和 surface。
- Win32 按 application、window、renderer、assets、services 归档。
- Win32 Application Host 分为 contract、state、backend、window、message loop、rendering、input。
- 增强 GDI 分为 cache、renderer、compositing、primitives、text。
- 增强 D2D 分为 cache、renderer、collection、drawing、effects、resources。
- 对共享大量私有原生状态的 Win32/Skia 叶文件使用同一拥有者模块内的 `include!`；
  只改变物理文件边界，不改变消息循环、资源所有权或缓存生命周期。
- 修正旧 GDI fallback 将浮点 UI 坐标传给 Win32 整数 API 的编译错误；转换统一发生在
  Win32 API 边界，默认 Feature 现可编译。

## 服务端匹配的客户端配置

- 使用 `liuguangss/scripts/dev/environment.mjs` 初始化
  `liuguangss/.local/dev/dev.env` 和本地 TLS 证书。
- 已生成被 Git 忽略的 `native/windows/client.toml`。
- 网关为 `https://localhost`；配置嵌入本地开发证书。
- Bootstrap X25519 公钥、Ed25519 key id 和公钥均从同一份服务端开发环境提取；
  私钥没有读取到客户端配置，也没有写入客户端仓库。
- 如果删除或重新生成 `liuguangss/.local/dev`，必须同步重新生成客户端配置。

## rust-skia 环境

- 解决 Unicode 工作区导致预编译包下载失败的问题。
- 本地缓存目录：
  `C:/Users/yys/AppData/Local/lgui/skia/0.99.0`
- 已缓存两套 rust-skia 0.99.0 Windows MSVC 产物：
  - GL/JPEG/PDF/SVG/TextLayout
  - D3D/GL/Vulkan/JPEG/PDF/SVG/TextLayout
- 用户级环境变量使用 Feature key 模板：
  `SKIA_BINARIES_URL=file://C:/Users/yys/AppData/Local/lgui/skia/0.99.0/skia-binaries-{key}.tar.gz`
- 基础 Winit + Skia 和 `lgui --all-features` 均已在默认 Unicode 工作区验证。

## 架构门禁

`tests/architecture.rs` 现在会检查：

- 顶层子系统和关键职责目录存在。
- Router、Core Runtime/Render、Host、Assets、Text、Widget 的旧单文件路径不存在。
- Portable Core、Renderer、Assets、Text 不依赖 Win32/Winit 类型。
- Command/Event 不依赖序列化、Store、Router 或平台传输。
- Win32 不依赖 Liuguang 业务模块。
- Skia 实现位于 `renderer/skia`，Winit surface adapter 保持平台边界。

## 验收记录

- `cargo fmt -p lgui -- --check`：通过。
- `cargo check -p lgui`：通过，默认 GDI Feature 已恢复编译。
- `cargo test -p lgui --no-default-features --quiet`：137 个单元测试和 15 个架构测试通过。
- `cargo check -p lgui --no-default-features --features backend-win32,renderer-d2d`：通过。
- `cargo check -p lgui --no-default-features --features backend-winit,renderer-skia`：通过。
- `cargo test -p lgui --all-features --quiet`：248 个单元测试和 15 个架构测试通过。
- `cargo check -p lgui-showcase --all-features`：通过；示例中的旧整数几何字面量已按
  当前 `f32` API 机械更新，布局值不变。
- `cargo check -p liugc --bin liugc`：`client.toml` 的解析、校验和加密嵌入阶段通过；
  随后 RC.EXE 因 Unicode 工程路径编译 Windows 资源失败。按本轮约定不把
  `native/windows` 的资源工具问题作为 LGUI 重构阻塞项。
- `git diff --check` 与 `git diff --cached --check`：通过。

## 收口规则

- 不再保留声称已完成但未验证的阶段。
- 后续行为修改、性能优化和 API 设计应独立于本次目录重构提交。
- 合并前再次执行格式化、最小/全 Feature 测试和 `git diff --check`。
