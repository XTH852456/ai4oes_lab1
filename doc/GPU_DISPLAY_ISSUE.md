# ch2 图像瞬间消失问题记录

## 1. 问题现象
- QEMU 图形窗口出现后，提示 `display output is not active`。
- 或者画面短暂出现后很快消失。
- 串口日志可能只看到前几批输出，无法稳定观察最终图像。

## 2. 根因分析

### 2.1 显示链路不匹配
- 早期实现使用了自定义 SBI 调用（`a7 = 0x42000`）尝试获取 framebuffer 地址。
- 当前 `tg-sbi` 教学实现没有该扩展，因此经常走到 `framebuffer unavailable` 分支，图形链路不稳定或不可用。

### 2.2 图形设备生命周期过短
- 在 VirtIO-GPU 模式下，如果显示驱动对象过早失活（例如对象被销毁、队列解绑），QEMU 端会把输出标记为 inactive。
- 这会表现为窗口仍在，但内容快速失效。

### 2.3 内核流程过早结束
- 若内核在绘制后很快关机，窗口会“看起来像闪一下就没了”。

## 3. 解决方案

### 3.1 统一到 VirtIO-GPU 显示路径
- 使用 `virtio-drivers` 驱动 GPU，并通过 MMIO 扫描设备初始化 framebuffer。
- 关键位置：`tg-rcore-tutorial-ch2/src/main.rs:313`（`find_gpu_mmio_header`）
- 关键位置：`tg-rcore-tutorial-ch2/src/main.rs:418`（`GpuDisplay::init` 调用）

### 3.2 让图形设备保持活跃
- 在 `rust_main` 中持有 `gpu_display`，并在批处理循环中持续使用同一个对象刷新每一帧。
- 关键位置：`tg-rcore-tutorial-ch2/src/main.rs:440`（每批次绘制 `display.draw_stage(stage)`）

### 3.3 最后一帧保持显示
- 演示结束后不立即关机，而是保持自旋，确保最后一帧稳定停留。
- 关键位置：`tg-rcore-tutorial-ch2/src/main.rs:455`（`Keeping last frame active...`）

### 3.4 QEMU 参数改为图形模式
- 使用 `virtio-gpu-device + gtk`，避免 `-nographic` 导致没有图形输出。
- 关键位置：`tg-rcore-tutorial-ch2/.cargo/config.toml:16`
- 关键位置：`tg-rcore-tutorial-ch2/.cargo/config.toml:17`

## 4. 当前结果
- 已能稳定看到 `virtio-gpu framebuffer ready` 日志。
- 图像分批渲染可见，最终画面可保持，不再“瞬间消失”。

## 5. 验证方法
1. 在 `tg-rcore-tutorial-ch2` 目录执行 `cargo run`。
2. 观察串口日志是否出现：`virtio-gpu framebuffer ready: ...`。
3. 观察 QEMU 窗口：图像应逐批出现并最终停留。
4. 若要退出，使用 `Ctrl+C` 或关闭 QEMU 窗口。

## 6. 回归检查清单
- 若再次出现 `display output is not active`：
  - 检查是否仍使用 `virtio-gpu-device`（而非 `-nographic`）。
  - 检查是否误改为旧版 `SBI 0x42000` framebuffer 路径。
  - 检查是否删除了“最后一帧保持活跃”的循环。
