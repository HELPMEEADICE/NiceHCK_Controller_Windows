# NiceHCK Controller Rust

NiceHCK Controller 的 Windows 原生 Rust 版本，使用 Slint 构建界面。

## 要求

- Windows 10/11
- Rust 1.85 或更新版本
- 已在 Windows 设置中配对的 NiceHCK / 原道蓝牙耳机

## 运行

```powershell
cargo run
```

程序依次尝试目标 UUID 的 RFCOMM 服务和蓝牙串口 SPP。设备匹配关键字为 `YUANDAO`、`OriG`、`NiceHCK` 和 `Controller`。

## 验证

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

日志保存在 `%LOCALAPPDATA%\NiceHCK Controller\logs\`。设置 `RUST_LOG` 可调整日志级别，例如 `RUST_LOG=debug`。

## 模块

- `src/protocol.rs`：命令构造、流式数据包解析和状态响应解码
- `src/models.rs`：设备状态、模式和固件能力
- `src/transport/`：Windows RFCOMM、串口 SPP 和设备发现
- `src/controller.rs`：单任务控制器状态机
- `ui/app.slint`：原生桌面界面
