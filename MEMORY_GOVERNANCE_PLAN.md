# LGUI 资源生命周期、缓存与内存治理重构计划

> 已完成的实施记录。长期有效的架构、公开 API 与验收规则位于
> `ARCHITECTURE.md`、`API.md` 和 `BASELINE.md`；本文保留设计背景和阶段证据。

## 1. 状态

- 总体状态：阶段 0-9 已完成，长期规则已转入正式文档
- 建立日期：2026-08-31
- 代码基线：`319f146`
- 主要范围：`native/lgui`
- 集成范围：`native/windows` 只负责提供应用级配置、持久缓存目录和设置界面
- 重构单位：继续保留单个 `lgui` crate
- 核心目标：让所有可重建资源有界、可观测、可回收，并允许调用方声明资源保留策略

阶段状态只允许使用：`未开始`、`进行中`、`已完成`、`阻塞`。阶段只有在其验收门槛全部
满足后才能标记为已完成。

## 2. 问题陈述

当前 `lgui` 已经包含图片、SVG、模糊、静态层、滚动栅格、文本、D2D、GDI、Skia、
Component、Host 和 Scene 等多种复用机制，但它们由不同模块独立管理：

- 部分缓存有字节预算和 LRU。
- 部分缓存只限制条目数。
- 部分缓存是没有预算的 `HashMap`。
- 部分缓存只能整体清理，不能按资源失效。
- 部分缓存只有窗口隐藏时才会释放。
- 各子系统的独立预算没有汇总为应用总预算。
- 当前 `MemoryAndDisk` 名称与实现不符，实际仍然保存在进程内存中。
- 当前无法区分任务管理器中看到的内存属于当前必需资源、缓存、临时峰值还是 GPU 资源。

单个缓存的 32 MiB、64 MiB 或 96 MiB 看似合理，但多个缓存同时存在时，进程可能保留
数百 MiB 可重建数据。同一资源在下载、解码、缩放、模糊、上传和合成期间还可能同时存在
多份副本。这样的行为背离原生客户端低开销、生命周期明确的目标，也容易被用户误认为内存
泄漏、异常后台计算或恶意行为。

本轮工作不是“增加图片缓存”，而是建立统一的资源生命周期和内存治理系统。

## 3. 不可妥协的设计原则

1. 任何缓存都不得无界增长。
2. 应用总预算优先于子系统各自的局部预算。
3. 当前可见且绘制必需的资源不得被错误归类为可淘汰缓存。
4. 不缓存不等于每帧重新下载、解码或栅格化。
5. 缓存策略必须描述保留范围、优先级和失效方式，不能只有一个含义模糊的布尔值。
6. 所有大额分配必须在分配前进行尺寸校验或预算预留。
7. 磁盘缓存只保存适合持久化的可重建数据，不保存原生句柄或无压缩巨型像素副本。
8. 资源离开场景、窗口隐藏、设备丢失和应用退出必须有明确回收路径。
9. 每个缓存必须提供占用、命中、未命中、淘汰、最大条目和清理统计。
10. 业务 backend 不负责 GUI 图片下载、解码、渲染缓存或淘汰。
11. `lgui` 不读取 Liuguang 业务配置，也不选择应用专属缓存目录。
12. 新增缓存不得以降低正确性、产生旧资源或隐藏更新为代价。

## 4. 范围与非目标

### 4.1 本轮范围

- CPU 编码资源、解码资源和中间像素。
- GDI、Direct2D 和 Skia 的可重建原生资源。
- SVG、Blur、Static Layer、Scroll Raster 和文本布局缓存。
- Component 输出、Host/Scene 可重建投影和非活动窗口资源。
- 统一预算、回收、统计、压力处理和诊断。
- 图片和栅格资源的逐资源保留策略。
- 可选的压缩资源磁盘缓存。
- Windows 客户端的内存档位、磁盘配额和清理入口。

### 4.2 非目标

- 不将 Store、登录凭据、Router History 或用户业务数据称为缓存并随意淘汰。
- 不通过清空所有组件 State 来降低内存。
- 不在本轮修改页面视觉设计。
- 不创建第二套渲染器、Runtime 或业务网络客户端。
- 不承诺操作系统工作集会在每次 Rust 对象释放后立即下降；验收同时观察内部驻留字节、
  private bytes、working set 和 GPU 占用。
- 不以磁盘缓存替代服务端 HTTP 缓存头、ETag 或内容版本。

## 5. 术语与内存分类

### 5.1 当前必需资源（Live Resource）

当前帧或当前可见窗口绘制所必需的对象，例如活动 Host Tree、当前 Scene、窗口后备表面和
可见 Compositing Layer。它们不属于缓存，不能为了满足缓存预算而直接删除。

### 5.2 可重建状态（Rebuildable State）

可以从组件、资源源数据或 Scene 重新生成的结构，例如组件已提交输出、文本布局结果和
已编译 Scene 片段。它们可以在内存压力下丢弃，但必须保留正确重建所需的身份和依赖信息。

### 5.3 缓存条目（Cache Entry）

为了避免重复下载、解码、布局、栅格化或上传而保留的可淘汰对象。每个条目必须有稳定 Key、
估算字节、最后使用时间、可达性、优先级和失效版本。

### 5.4 临时分配（Transient Allocation）

下载缓冲、解码缓冲、缩放副本、模糊中间图和上传暂存区。它们不进入 LRU，但必须受并发、
单任务上限和总预留字节约束。

### 5.5 持久缓存（Persistent Cache）

跨进程保存的压缩资源和元数据。持久缓存有单独磁盘配额、TTL、校验和与原子写入规则，不能
把文件大小计入内存驻留预算。

## 6. 当前实现基线

下表是开始实施前必须保留并持续更新的缓存总账。

| 域 | 当前所有者 | 当前策略 | 已知问题 | 目标 |
| --- | --- | --- | --- | --- |
| 便携异步图片字节 | `assets/cache.rs` | Winit 默认 64 MiB、按使用时间淘汰 | 策略由后端硬编码，缺少统一统计 | 注册到总预算并支持逐资源策略 |
| Win32 图片下载字节 | `platform/win32/assets/image_cache.rs` | 按 URL/路径保存在全局 Map | 无总预算、无单项失效、无可达性 | 应用作用域、有界 LRU、Single Flight |
| Win32 GDI+ 解码图片 | 同上 | UI 线程本地 Map | 无预算、可能长期保留原生图像 | 按估算像素字节和可达性回收 |
| Enhanced 解码图片 | `renderer/enhanced/image.rs` | UI 线程本地 Map | 无预算，可能与其他图片缓存重复 | 合并所有权或纳入统一预算 |
| SVG Bitmap | `platform/win32/assets/svg.rs` | 按图标、尺寸、颜色缓存 | 无预算；尺寸和颜色组合会扩张 | 有界 LRU，当前 Scene 可达性优先 |
| Blur 结果与源栅格 | `renderer/enhanced/blur.rs` | 三个独立 Map | 无预算，源图和多级结果可能重复 | 统一计量，中间结果低优先级 |
| Static Layer Memory | `renderer/enhanced/static_layer.rs` | 默认 64 MiB、LRU | 预算与应用总预算无协调 | 由 Governor 分配域预算 |
| Static Layer “Disk” | `static_layer_raster_cache.rs` | 进程全局 Map | 不是真磁盘、无预算、可能重复保存像素 | 删除伪磁盘路径或接入真实 Store |
| Scroll Raster Command | `core/scene/render/scene.rs` | 最多 8 个快照 | 只有条目数，没有字节统计 | 同时限制条目和字节 |
| Scroll Raster Tile | Windows `scroll_area.rs` | 默认 32 MiB 配置 | 页面级预算可能与其他域叠加 | 使用共享预算和可见 Tile Pin |
| GDI Bitmap | GDI Renderer | 64 MiB LRU | Overlay、字体和部分 Surface 未统一计量 | GDI 域统一统计和回收 |
| D2D Bitmap | D2D Renderer | 至少 32 MiB，按视口扩展并保留可达 Key | Frame Cache、Brush、Layer 未统一展示 | CPU/GPU 分离统计，总预算协调 |
| Skia Image/Text | Skia Renderer | 默认 96 MiB，CPU/GPU 分配 | 与其他 `lgui` 缓存没有总账 | 接入同一 Profile 和诊断快照 |
| Component 输出 | ComponentTree | props/依赖相等时保留 | 没有容量估算；非活动树释放规则不统一 | 区分 State 与可重建输出 |
| Host/Scene | UiSession/Host | 结构共享和增量保留 | 没有驻留量和非活动窗口统计 | 生命周期计量，不作为普通 LRU 误删 |
| 系统诊断采样 | Win32 Diagnostics | 默认 1 秒采样缓存 | 占用很小但未登记 | 登记为低成本固定缓存 |
| Steam 头像文件 | Windows backend | 应用缓存目录 PNG | 不受 `lgui` 配额和统一清理管理 | 应用业务缓存继续独立，明确不计入 GUI 缓存 |

阶段 0 必须重新核对此表。发现新的 Map、原生资源池或重建快照时，必须先登记再修改。

## 7. 目标架构

```text
Application
  ├─ MemoryOptions / MemoryProfile
  ├─ MemoryGovernor
  │    ├─ global soft/hard budgets
  │    ├─ temporary allocation reservations
  │    ├─ pressure and background trimming
  │    └─ aggregate diagnostics
  ├─ CacheRegistry
  │    ├─ assets / encoded / decoded images
  │    ├─ svg / blur / text
  │    ├─ static layer / scroll raster
  │    ├─ gdi / d2d / skia native resources
  │    └─ rebuildable component / host / scene projections
  └─ optional PersistentCacheStore

UiSession / Renderer / Asset Runtime
  ├─ own concrete entries and native handles
  ├─ report usage and reachability
  ├─ obey assigned budget
  └─ release entries through native owner thread
```

### 7.1 所有权边界

- `Application` 持有一个 `MemoryGovernor` 和应用级配置。
- 每个具体缓存继续由最了解其线程和 native drop 规则的子系统持有。
- Governor 不直接跨线程析构 HWND、HBITMAP、D2D 或 Skia 对象，只发出有界 Trim 请求。
- `UiSession` 报告活动窗口、当前帧和组件/Scene 生命周期。
- 平台 Renderer 报告 CPU 和 GPU 估算，并在正确线程执行回收。
- 应用可以提供持久缓存目录或 `PersistentCacheStore`；未提供时 `lgui` 只使用内存。
- Windows backend 可以提供路径和用户设置，但不得接管资源下载和渲染缓存。

### 7.2 计划中的源码结构

```text
src/memory/
├─ mod.rs
├─ options.rs       # MemoryOptions、MemoryProfile 和预算解析
├─ policy.rs        # 内部保留级别、优先级和资源限制
├─ governor.rs      # 总预算、预留、Trim 和压力决策
├─ registry.rs      # CacheDomain 注册与生命周期
├─ stats.rs         # 统一快照和诊断模型
├─ pressure.rs      # 后台、显式和平台内存压力事件
└─ persistent.rs    # 可选持久 Store 契约和文件实现
```

各子系统仍保留自己的 `cache.rs`，但必须通过 `memory` 注册，不能再形成无法汇总的孤立缓存。

## 8. 统一策略模型

### 8.1 内部保留级别

```rust,ignore
pub enum RetentionClass {
    Frame,
    WhileVisible,
    Scene,
    Session,
    Persistent,
}

pub enum CachePriority {
    Low,
    Normal,
    High,
}
```

- `Frame`：只保留到当前帧结束。
- `WhileVisible`：资源所属元素仍在活动 Scene 时保留；离开后立即成为首批淘汰候选。
- `Scene`：允许在同一页面或窗口 Scene 内跨帧复用。
- `Session`：允许跨页面保留到 Application Session 结束，但仍受 LRU 和硬预算约束。
- `Persistent`：压缩源数据可以写磁盘；内存副本仍受预算约束。

当前帧可见资源通过 Pin 表达，而不是把 `High` 误当成永不淘汰。任何普通调用方都不能创建
永久 Pin。

### 8.2 面向不同资源的公开策略

共享的内部词汇不意味着所有公开 API 使用同一个枚举。

- 图片使用 `ImageCachePolicy` 和 `ImageDecodePolicy`。
- 静态/滚动栅格使用 `RasterCachePolicy`。
- Component 使用保留式执行语义，不提供 `cache_ele(bool)`。
- 文本、SVG、Blur 和 native renderer 默认由框架自动管理，只在高级 API 中开放覆盖。

计划中的图片 API：

```rust,ignore
let request = ImageRequest::new(UiImageSource::url(url))
    .cache_policy(ImageCachePolicy::Persistent {
        max_age: Duration::from_secs(7 * 24 * 60 * 60),
        revalidate: true,
    })
    .decode_policy(ImageDecodePolicy::FitTarget(PhysicalSize::new(128, 128)))
    .priority(CachePriority::High);

image(rect, request).fit(ImageFit::Cover)
```

一次性资源：

```rust,ignore
ImageRequest::new(source)
    .cache_policy(ImageCachePolicy::WhileVisible)
    .priority(CachePriority::Low)
```

`NoStore` 的语义是“不在元素生命周期外保留”，而不是每帧重新下载和解码。

计划中的栅格 API：

```rust,ignore
StaticLayerSpec::new(StaticLayerSource::runtime())
    .cache_policy(RasterCachePolicy::Memory {
        retention: RetentionClass::Scene,
        priority: CachePriority::Normal,
    })
    .revision("profile-card-v2")
```

### 8.3 Component 语义

`component(props, render)` 继续负责 Hook、依赖和已提交输出复用。必须区分：

- Component State、Effect 和订阅是正确性状态，不能作为普通缓存淘汰。
- 已提交 Element 输出是可重建状态，可以在压力下丢弃并标记组件重新执行。
- Scene/Raster 投影是可重建资源，可以独立淘汰。
- 已卸载路由和窗口必须递归释放整个组件生命周期。

需要缓存组件的像素时使用 Static/Compositing/Raster Layer，而不是隐藏在通用
`cache_ele(element, true)` 中。

## 9. 总预算与内存档位

Governor 同时维护软预算和硬预算：

- 软预算：超过后在帧尾按优先级和 LRU 回收，不阻塞当前绘制。
- 硬预算：新建可重建缓存前必须预留；预留失败则降级、淘汰或不保存。
- 当前可见资源可以形成明确的 `pinned_overflow_bytes`，但必须在诊断中单独报告。
- 单个超大资源不得依赖“缓存至少保留一个超限条目”的旧规则。

初始候选档位如下，阶段 0 可以基于固定场景测量调整一次；阶段 1 冻结后不得无记录改变：

| 档位 | CPU 可重建缓存软预算 | GPU/native 缓存软预算 | 临时分配上限 | 磁盘默认配额 |
| --- | ---: | ---: | ---: | ---: |
| LowMemory | 64 MiB | 64 MiB | 32 MiB | 128 MiB |
| Balanced | 128 MiB | 128 MiB | 64 MiB | 512 MiB |
| Performance | 256 MiB | 256 MiB | 128 MiB | 2 GiB |

硬预算初始为软预算的 125%，但最多额外增加 64 MiB。当前必需窗口表面不计入缓存预算，
必须作为 `live_bytes` 单独显示。

默认单资源限制候选值：

- 网络编码资源：32 MiB。
- 解码像素：64 MiB 或 16,777,216 像素，以先达到者为准。
- SVG 输出：不超过目标 Surface 尺寸和当前档位单项上限。
- 并行大资源解码：LowMemory 为 1，Balanced/Performance 为 2。
- 未提供目标尺寸的大图必须在解码前读取尺寸并执行拒绝或降采样策略。

## 10. 缓存条目与淘汰规则

每个受管条目至少记录：

```text
domain / key / version
retention / priority
resident_bytes / reserved_bytes
created_tick / last_used_tick / last_visible_frame
hit_count / rebuild_cost
state: loading | ready | failed
pin_count / owner_session / owner_window
```

### 10.1 淘汰顺序

1. 已过期或版本不匹配条目。
2. 离开 Scene 的 `WhileVisible` 条目。
3. 不可见窗口中的低优先级条目。
4. 未 Pin 的低优先级 LRU。
5. 未 Pin 的普通优先级 LRU。
6. 可重建的 Component 输出和 Scene 投影。
7. 内存中的 Persistent 源副本；磁盘文件可以继续存在。

可见 Pin、正在呈现的 Surface 和正在被 native 命令使用的资源不能在当前帧中被释放。

### 10.2 Reachability

- Scene 编译时收集本帧资源 Key。
- Renderer 在提交成功后更新 `last_visible_frame`。
- 连续若干帧不可达后，资源从 Pin 集合转为可淘汰。
- Popup、辅助窗口和多窗口分别上报可达性，不能只观察主窗口。
- 设备丢失时 native 资源立即失效；CPU 源缓存是否保留由预算决定。

### 10.3 Single Flight 与失败缓存

- 同一 Key 同一版本只允许一个下载、读取、解码或栅格任务。
- 其他调用方共享 Loading 状态和完成通知。
- 失败结果使用短时间退避，防止每帧重新发起失败请求。
- 失败缓存不进入长期 LRU，URL 或版本变化后立即失效。

## 11. 控制临时内存峰值

缓存预算不能解决单次处理链的瞬时峰值。所有大资源任务必须遵循：

1. 在读取或解码前预留预计字节。
2. 先读取尺寸和格式元数据，再决定完整解码、降采样或拒绝。
3. 优先按最终显示目标解码，头像不得长期保留远大于显示尺寸的 RGBA。
4. 中间 Vec、RasterImage 和上传缓冲必须在最后使用点后立即 Drop。
5. 上传 native/GPU 资源后，只有策略确实需要时才保留 CPU 解码像素。
6. Blur 尽量复用一个工作缓冲，并限制同时存在的源、横向和纵向中间副本。
7. 取消或失去所有等待者的异步任务应尽早停止。
8. 任务完成前不得把未校验的条目计入可用缓存。

测试必须覆盖压缩体积小但解码体积巨大的资源，防止解压炸弹和整数溢出。

## 12. 持久缓存设计

### 12.1 契约

```rust,ignore
pub trait PersistentCacheStore: Send + Sync + 'static {
    fn get(&self, key: &PersistentCacheKey) -> Result<Option<PersistentEntry>, CacheStoreError>;
    fn put(&self, entry: PersistentEntry) -> Result<(), CacheStoreError>;
    fn remove(&self, key: &PersistentCacheKey) -> Result<(), CacheStoreError>;
    fn trim_to(&self, budget_bytes: u64) -> Result<PersistentCacheStats, CacheStoreError>;
    fn clear(&self, namespace: Option<&str>) -> Result<(), CacheStoreError>;
}
```

`lgui` 提供基于文件系统的默认实现，但目录必须由 Application 显式提供。没有 Store 时，
`Persistent` 安全退化为有界 Memory/WhileVisible，而不是 panic。

### 12.2 文件与元数据规则

- 文件名使用 namespace、规范化 Key 和内容 Hash，不直接使用 URL 作为路径。
- 原子临时文件写入后 rename。
- 元数据记录内容长度、Hash、MIME、创建/访问时间、TTL、ETag 和 Last-Modified。
- 读取时校验长度和 Hash；损坏条目删除并视为 Miss。
- 配额淘汰使用最近访问时间，并限制单文件大小。
- 只持久化公开且调用方允许的数据；鉴权响应、Token 和敏感用户数据默认禁止持久化。
- 磁盘缓存保存压缩源或明确的便携格式，不保存 HBITMAP、D2D Bitmap、Skia Image 或
  大型裸 BGRA。

### 12.3 修正 `MemoryAndDisk`

- 在真实 Store 落地前，将现有 `MemoryAndDisk` 标记废弃并按 `Memory` 执行。
- 删除 `static_layer_raster_cache.rs` 中伪磁盘的全局无界 Map。
- 如果静态层确实需要持久化，必须通过统一 Store、配额和版本元数据实现。
- 当前游戏背景调用方必须迁移到明确的新策略，不能依赖旧名称。

## 13. 内存压力与生命周期事件

Governor 接受以下事件：

- `FrameCommitted`
- `WindowHidden` / `WindowShown`
- `AllWindowsHidden`
- `SessionUnmounted`
- `RendererDeviceLost`
- `ThemeOrScaleChanged`
- `MemoryPressure::{Moderate, Critical}`
- `ExplicitTrim`
- `ApplicationShutdown`

建议行为：

| 事件 | 行为 |
| --- | --- |
| 帧提交 | 更新 Reachability，超过软预算时渐进淘汰 |
| 单窗口隐藏 | 释放该窗口 native Surface 和低优先级 Scene 缓存 |
| 全部窗口隐藏 | 清理 WhileVisible、临时缓存和可重建 native 资源，保留最小热数据 |
| Session 卸载 | 释放该 Session Pin、组件输出和 Scene 投影 |
| 设备丢失 | 丢弃全部设备相关缓存，保留受预算约束的便携源 |
| Moderate | 回收到软预算的 75% |
| Critical | 只保留当前可见 Pin 和正确性状态 |
| 显式清理 | 按域或全部清理，并返回清理前后统计 |

现有 `background_memory_optimization(true)` 迁移为向 Governor 发送生命周期事件，不再手工
枚举多个 `clear_*_cache()`。

## 14. 失效与版本模型

必须提供：

```rust,ignore
invalidate(CacheKey)
invalidate_namespace("avatars")
trim(CacheDomain, target_bytes)
clear(CacheScope::Memory)
clear(CacheScope::Persistent)
clear_all_rebuildable()
```

规则：

- `UiImageSource::Bytes` 继续使用 `key + version`。
- 文件资源 Key 必须考虑路径和显式版本；可选使用 mtime/size 作为辅助，不依赖它保证正确性。
- URL 资源使用规范化 URL、调用方 namespace 和显式版本。
- HTTP Persistent 缓存优先使用 ETag/Last-Modified；没有验证器时使用 TTL。
- Static Layer 继续使用 `revision + child_signature + size + scale`。
- Theme、字体、DPI、Renderer 或颜色空间变化必须只失效相关域。

全局清理函数可以作为兼容 Facade 保留，但最终必须委托给统一 Registry。

## 15. 诊断与可观测性

公开诊断快照至少包含：

```text
profile / global soft and hard budgets
live_bytes / rebuildable_bytes / cache_bytes / transient_reserved_bytes
cpu_bytes / gpu_estimated_bytes / persistent_bytes
pinned_bytes / pinned_overflow_bytes
per-domain entries, bytes, hits, misses, evictions, rebuilds
largest entries
in-flight downloads/decodes/rasters and reservations
last trim reason, duration and released bytes
```

必须同时提供：

- 结构化 Rust API。
- 现有 diagnostics runtime 的只读查询与显式 Trim 命令。
- 可选 trace 事件：reserve、store、hit、miss、evict、trim、oversize、pressure。
- Debug 构建中的预算不变量断言。

任务管理器中的 Working Set 可能受分配器和 Windows 工作集策略影响。测试报告必须并列展示：

- Governor 报告的可重建驻留字节。
- 进程 Working Set 和 Private Bytes。
- Renderer 能取得时的 GPU Resource Bytes。
- 最近一次 Trim 前后变化。

任何未进入上述分类的大额增长都视为缺陷，而不是“统计不到”。

## 16. 应用和用户设置

Application API：

```rust,ignore
Application::new()
    .memory_options(MemoryOptions::balanced())
    .persistent_cache(FileCacheStore::new(cache_dir))
```

Windows 设置页面最终提供：

- 内存模式：低内存、平衡、性能。
- 是否启用磁盘资源缓存。
- 磁盘缓存配额。
- 清理内存缓存。
- 清理磁盘缓存。
- 当前缓存占用摘要。

用户设置控制总体资源使用，不暴露每个头像或图标的业务级开关。逐资源策略由组件作者声明。
没有显式设置时使用 `Balanced`，低可用内存设备可以在启动时自动降级，但必须在诊断中显示。

## 17. 分阶段实施计划

### 阶段 0：冻结场景、盘点与测量基线

状态：已完成

工作内容：

- 更新第 6 节总账，搜索所有 Map、native pool、retained snapshot 和文件缓存。
- 为每个条目记录所有者、线程、Key、估算方法、清理入口和调用方。
- 固定 GDI、D2D、Skia 的相同场景、窗口、DPI、Release 构建和操作脚本。
- 固定场景：冷启动登录、快速登录头像、主页面、商店长列表、主题切换、Dialog、路由往返、
  多窗口隐藏/恢复、大图、唯一 SVG/Blur 压力。
- 采集稳定 30 秒、操作峰值、页面退出、全窗口隐藏和重新显示后的指标。
- 确认初始三档预算；任何调整写回第 9 节。

验收门槛：

- 所有已知缓存均在总账中。
- 能解释基线进程内存的主要组成。
- 测试脚本可重复，结果包含内部与 OS 指标。
- 预算数值冻结，阶段 1 可以据此定义 API。

### 阶段 1：建立内存契约、配置与 Registry

状态：已完成

工作内容：

- 创建 `memory/` 模块。
- 定义 Domain、Profile、Options、Retention、Priority、Usage 和 TrimReason。
- Application 持有 Governor 和 Registry。
- 定义缓存注册、线程内 Trim 回调和统一统计契约。
- 保留现有行为，仅将已有统计接入总账。
- 增加架构测试，禁止新增未登记的无界缓存。

验收门槛：

- 不启用任何平台特性时契约可以编译测试。
- GDI、D2D、Skia、Winit 可以注册自己的域。
- Registry 生命周期严格属于 Application，不泄漏业务或平台类型。
- 此阶段不引入视觉或缓存命中行为变化。

### 阶段 2：先完成可观测性

状态：已完成

工作内容：

- 为当前所有缓存补充条目字节、命中、未命中、淘汰和最大条目统计。
- 计量 Component 输出、Host、Scene 和活动 native Surface。
- 增加临时分配 Reservation 统计，但暂不强制拒绝。
- 接入 diagnostics runtime 和固定场景报告。

验收门槛：

- 每个总账域在诊断快照中都有条目。
- 全部域之和与已知分配之间的差异有解释和阈值。
- 页面退出和窗口隐藏后的 retained 域变化可见。
- 无统计的新增缓存被架构测试拒绝。

### 阶段 3：消除无界缓存并统一总预算

状态：已完成

工作内容：

- 将 Win32 图片字节、解码图片、SVG 和 Blur 改为有界缓存。
- 将 GDI Overlay/Font、D2D Frame/Brush/Layer 纳入生命周期或预算统计。
- 删除伪磁盘全局 Map；旧 `MemoryAndDisk` 暂时降级为 `Memory`。
- Scroll Raster 同时限制条目与字节。
- Governor 分配域预算并执行全局 Trim。
- 现有 `clear_*` API 委托 Registry。

验收门槛：

- 源码中不存在未登记且无边界的重建资源 Map。
- 压力场景中内部 cache bytes 达到平台后形成平台线而非持续增长。
- 超过软预算会淘汰，超过硬预算不会继续保存普通缓存。
- 当前可见资源不会因淘汰产生空白或 use-after-free。

### 阶段 4：逐资源策略与 Reachability

状态：已完成

工作内容：

- 引入 `ImageRequest`、图片策略、优先级、Decode Policy 和 namespace/version。
- Scene 提交统一资源 Reachability。
- 实现 `Frame`、`WhileVisible`、`Scene` 和 `Session` 内存语义。
- 迁移头像、普通图片、一次性图标、背景和动态字节图片。
- Static Layer 迁移到明确的新 Raster Policy。
- Component 保留语义和 Raster 缓存 API 保持分离。

验收门槛：

- 头像、背景和一次性图标的策略有测试和真实调用方。
- `NoStore`/`WhileVisible` 不会导致每帧重新解码。
- 相同 Key 的并发请求只有一个任务。
- 离开 Scene 的低优先级资源在预算压力下优先回收。

### 阶段 5：限制峰值、目标尺寸解码与任务背压

状态：已完成

工作内容：

- 实现 Reservation 和解码并发限制。
- 加入编码大小、像素数量、解码字节和目标 Surface 限制。
- 支持按目标尺寸降采样或明确拒绝。
- 优化 Blur、缩放和 native 上传的中间副本生命周期。
- 任务取消和失败退避接入缓存状态机。

验收门槛：

- 超大压缩图、极端尺寸和损坏格式不会造成无界峰值或崩溃。
- 头像只保留满足目标清晰度的解码尺寸。
- 并发加载大量资源时 transient reserved bytes 不超过硬上限。
- 失败资源不会每帧重复请求。

### 阶段 6：真实持久缓存

状态：已完成

工作内容：

- 实现 `PersistentCacheStore` 和应用提供目录的文件 Store。
- 实现原子写入、Hash 校验、TTL、ETag、Last-Modified 和磁盘 LRU。
- Persistent 图片只持久化允许的压缩响应。
- 恢复明确的持久策略，删除旧 `MemoryAndDisk`。
- 添加 Windows 设置和清理命令。

验收门槛：

- 重启应用后允许持久的头像可以命中磁盘。
- 同 URL 头像更新时通过验证器或 TTL 刷新。
- 损坏文件自动删除，磁盘配额严格执行。
- 没有 Store 时功能正常降级。
- 敏感响应不会被默认写入磁盘。

### 阶段 7：Component、Host、Scene 与多窗口生命周期

状态：已完成

工作内容：

- 计量并可释放 Component 的可重建输出，不删除 State/Effect 正确性数据。
- 明确路由卸载、隐藏窗口、辅助窗口关闭和 Session 销毁行为。
- Host/Scene 共享快照记录活跃引用，不长期保留已卸载分支。
- Compositing Layer、Backbuffer 和设备资源响应窗口及设备生命周期。
- `background_memory_optimization` 改为 Governor Profile/Event Facade。

验收门槛：

- 路由反复进入退出后 Component/Host/Scene 条目形成平台线。
- 关闭辅助窗口后，该窗口 native 资源和 Pin 归零。
- 全部窗口隐藏后可重建内存降至定义的后台目标。
- 状态恢复、Effect cleanup 和多窗口共享 Store 行为不回归。

### 阶段 8：设置、压力适配和用户可见诊断

状态：已完成

工作内容：

- 接入 LowMemory/Balanced/Performance。
- 支持平台内存压力和显式 Trim。
- 设置页面提供档位、磁盘缓存、配额和清理。
- Diagnostics 提供域明细、最大条目和最近 Trim。
- 记录设置变更后的即时预算收缩与异步清理行为。

验收门槛：

- 切换到低内存模式会在有界时间内收缩到新软预算。
- 用户可以清理内存和磁盘缓存并看到结果。
- 设置持久化不属于 `lgui`，但 Application 配置能正确恢复。
- 生产构建的诊断不会本身形成显著内存负担。

### 阶段 9：收口、兼容删除与长期文档

状态：已完成

工作内容：

- 删除所有临时适配器、旧伪磁盘策略和未登记清理路径。
- 更新 `ARCHITECTURE.md`、`API.md`、`BASELINE.md` 和示例。
- 固化 feature matrix、压力测试和发布前交互测试。
- 完成最终内存报告并删除本文档的临时状态。

验收门槛：

- 第 21 节完成定义全部满足。
- 兼容清单归零。
- 长期文档描述与实现一致。
- 工作记录包含每阶段提交和测试结果。

## 18. 测试与基准矩阵

### 18.1 单元测试

- 软/硬预算和跨域分配。
- Priority、LRU、Reachability 和 Pin。
- 单个超大条目、预留失败和降级。
- Single Flight、取消、失败退避和版本失效。
- TTL、ETag、损坏文件、磁盘 LRU 和原子写入。
- native drop 必须在所有者线程执行。
- Component 输出淘汰后正确重新执行且 State 不丢失。

### 18.2 集成场景

- 登录页快速账户预览和头像。
- 多个页面反复显示相同头像。
- 商店长列表连续滚动和返回。
- Dialog 打开、动画、关闭和重复打开。
- 主题、DPI、缩放和窗口尺寸切换。
- 主窗口、好友窗口和聊天窗口同时存在。
- 所有窗口隐藏后恢复。
- Direct2D 设备丢失与恢复。
- GDI、D2D 和 Skia 相同 Scene。

### 18.3 压力场景

- 数千个唯一 URL、SVG 尺寸/颜色组合和 Blur Key。
- 小压缩体积、超大解码尺寸图片。
- 多个大图并发进入和快速离开页面。
- 反复 Route mount/unmount 1,000 次。
- 窗口创建/关闭、DPI 迁移和 Renderer 切换。
- 磁盘缓存达到配额、文件损坏和目录不可写。

### 18.4 固定指标

- 冷启动与热启动 time-to-first-present。
- event-to-present p50/p95。
- 稳态 Working Set、Private Bytes 和 GPU 估算。
- 操作峰值和峰值持续时间。
- 页面退出、窗口关闭和全部隐藏后的回收量。
- 各域命中率、淘汰率、重建时间和最大条目。
- 30 秒和 10 分钟压力运行中的内存斜率。

## 19. 架构门禁

自动测试最终必须检查：

- 新缓存必须注册 `CacheDomain`、字节估算、Trim 和 Stats。
- 禁止在可重建资源路径新增无预算的进程全局/线程本地 Map。
- `MemoryAndDisk` 和伪磁盘 raster cache 不存在。
- Portable `memory` 不依赖 Win32、Winit、业务 backend 或页面。
- Win32/D2D/GDI/Skia 只实现适配器，不拥有第二个 Governor。
- Application 设置目录可以注入，但 `lgui` 不读取 `client.toml`。
- 业务 backend 不包含 GUI 图片下载或 Renderer 缓存逻辑。
- Feature 关闭时不编译无关平台或持久缓存实现。

## 20. 临时兼容清单

清单已归零。旧清理 Facade、分散预算、伪磁盘 Raster、线程本地清理旁路和隐式 Auto
策略均已删除或由最终契约直接替代。架构测试禁止这些路径重新出现；后续不保留本轮迁移
标记或兼容适配器。

## 21. 最终完成定义

只有以下条件全部成立，本轮重构才算完成：

- 所有可重建缓存都有 Domain、字节统计、软/硬预算和回收入口。
- 不存在未登记的无界图片、SVG、Blur、Raster、文本或 native 资源 Map。
- 应用总缓存预算可以约束多个子系统的合计，而不是只约束单个 Map。
- 当前必需资源、可重建状态、缓存、临时分配和磁盘文件可以分别统计。
- 图片、静态层和滚动栅格支持明确的保留策略和版本失效。
- `NoStore`/`WhileVisible` 不会造成每帧重复下载或解码。
- Component 正确性状态不会因缓存回收丢失；可重建输出可以回收。
- 同一资源的并发加载、解码或栅格化会合并。
- 单资源和临时并发限制可以阻止大资源造成无界峰值。
- 伪 `MemoryAndDisk` 实现已删除；真实磁盘缓存有配额、校验、TTL 和隐私规则。
- 窗口隐藏、Session 卸载、设备丢失和应用退出都有自动回收测试。
- LowMemory/Balanced/Performance 在 GDI、D2D 和 Skia 下行为一致。
- 压力测试中的可重建内存形成平台线，没有持续正斜率。
- 用户可以查看摘要、切换档位并清理内存/磁盘缓存。
- 本轮迁移标记搜索结果为零。
- `ARCHITECTURE.md`、`API.md` 和 `BASELINE.md` 已同步。
- 完整 Feature Matrix、Windows 客户端测试和 `git diff --check` 通过。

## 22. 每阶段验收命令

每阶段至少执行：

```powershell
cargo fmt --all -- --check
cargo check -p lgui --no-default-features
cargo test -p lgui --no-default-features --quiet
cargo test -p lgui --quiet
cargo check -p lgui --no-default-features --features renderer-gdi
cargo check -p lgui --no-default-features --features renderer-d2d
cargo check -p lgui --no-default-features --features backend-winit
cargo check -p liugc --bin liugc
cargo test -p liugc --bin liugc frontend:: --quiet
git diff --check
```

涉及 Skia、持久缓存或 Windows native 资源时，必须增加对应 Feature 和交互测试。Unicode 路径
导致 rust-skia 工具问题时，可以使用临时 ASCII 盘符执行验证，但不能因此跳过 Skia。

## 23. 风险与应对

| 风险 | 应对 |
| --- | --- |
| 预算过低导致频繁重建和卡顿 | 软/硬预算分离；记录重建成本；压力测试命中率和 p95 |
| native/GPU 字节只能估算 | 统一估算规则并标记 estimated；可用时读取后端真实统计 |
| 可见资源本身超过预算 | 单独报告 pinned overflow；降采样或拒绝缓存，不伪装成普通缓存 |
| 跨线程 Trim 造成错误析构 | Governor 发请求，缓存所有者线程执行 Drop |
| 磁盘缓存泄露敏感数据 | 默认仅公开资源可持久化；namespace 明确允许；不缓存鉴权响应 |
| API 过度复杂 | 普通组件默认 Auto；只在高级资源上暴露策略；不提供通用 bool |
| 淘汰导致视觉闪烁 | 当前 Scene Pin；后台渐进 Trim；重建完成后原子替换 |
| 工作集释放不及时引起误解 | 同时报告内部字节与 OS 指标；只在合适生命周期请求工作集 Trim |
| 计划只覆盖图片 | 架构门禁要求所有可重建域注册；总账每阶段更新 |

## 24. 变更控制与记录

常规实施应为每阶段保留可审计提交，并在本文档追加以下信息。本次按任务边界一次性完成且
明确不提交，因此实施记录使用同一未提交工作树并保留完整差异与测试结果：

- 阶段状态和提交 Hash。
- 实现的计划条目。
- 新增、迁移和删除的 Cache Domain。
- 预算或公开 API 变化。
- 执行的测试与基准结果。
- 新增和删除的兼容编号。
- 尚未满足的退出条件。

如果实现发现计划不正确，必须：

1. 停止扩展受影响实现。
2. 用代码、测试或测量说明冲突。
3. 先修改本文档中的目标、阶段和验收标准。
4. 单独提交计划修订。
5. 再继续实现。

不得通过增加未登记全局缓存、无界 Map、业务 backend 特例或静默提高预算绕过问题。

## 25. 实施记录

| 阶段 | 状态 | 提交 | 验收摘要 |
| --- | --- | --- | --- |
| 0 基线盘点 | 已完成 | 未提交（按任务要求） | 缓存总账、所有者线程、固定场景与三档预算已冻结；OS 指标由 diagnostics 固定采样。 |
| 1 契约与 Registry | 已完成 | 同上 | 新增 portable `memory` 模块、Application 级 Governor、Domain 注册与生命周期句柄。 |
| 2 可观测性 | 已完成 | 同上 | 各域统一字节/命中/淘汰/最大项统计，Component/Host/Scene、Reservation 与 OS 指标进入快照。 |
| 3 有界缓存与总预算 | 已完成 | 同上 | 图片、SVG、Blur、Raster、GDI、D2D、Skia 均受共享预算与 Trim 约束，伪磁盘 Map 已删除。 |
| 4 逐资源策略 | 已完成 | 同上 | `ImageRequest`、Raster Policy、priority/version/sensitivity 与多窗口 Reachability 已贯通 Scene。 |
| 5 峰值与背压 | 已完成 | 同上 | 单资源校验、目标尺寸解码、任务 Reservation、并发限制、single-flight 与失败退避已测试。 |
| 6 持久缓存 | 已完成 | 同上 | 文件 Store 具备原子写、SHA-256、TTL、验证器、磁盘 LRU、隐私和重启命中测试。 |
| 7 Retained 生命周期 | 已完成 | 同上 | Component 输出可重建且 State 保留；窗口/Session/设备事件统一 Trim，native 释放回所属 UI 线程。 |
| 8 设置与压力适配 | 已完成 | 同上 | Windows 设置 v6、档位/配额/清理界面、完整 MemorySnapshot、HUD 与调试命令已接入。 |
| 9 收口 | 已完成 | 同上 | 旧清理旁路和兼容层归零，长期文档、架构门禁与 feature 验收矩阵已固化。 |

最终验收以 `BASELINE.md` 为长期命令源。自动化覆盖预算、逐资源淘汰、持久缓存、目标尺寸
解码、多窗口可达性、owner-thread Drop、Component 重建、GDI/D2D/Skia 和 Windows 设置迁移；
发布候选包的硬件数值按该文档定义的固定交互场景采集，不把特定机器数值固化为跨机器阈值。
