# NiceHCK Controller

NiceHCK / 原道蓝牙耳机的 Windows 原生桌面控制器，使用 Rust 和 Slint 构建。

## 功能

- 自动发现已配对设备，优先使用原生 RFCOMM，失败后回退到串口 SPP
- ANC、EQ、AAC/LHDC/SBC 编码切换
- 游戏模式和低延迟模式
- 左耳、右耳和充电盒电量显示
- 根据固件版本限制不支持的 EQ 和编码选项
- 收发数据与错误日志

## 环境

- Windows 10/11
- Rust 1.85 或更新版本
- 已在 Windows 设置中配对的 NiceHCK / 原道耳机

## 运行

```powershell
cd rust
cargo run
```

## 构建

```powershell
cd rust
cargo build --release
```

生成的程序位于 `rust/target/release/nicehck-controller.exe`。

## 验证

```powershell
cd rust
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## 项目结构

```text
rust/
├─ src/
│  ├─ controller.rs          # 异步控制器与设备状态机
│  ├─ models.rs              # 状态、模式和固件能力
│  ├─ protocol.rs            # 协议编解码与流解析
│  └─ transport/             # RFCOMM、串口 SPP 与设备发现
├─ tests/                    # 协议测试
└─ ui/app.slint              # Slint 原生界面
```

运行日志按天写入 `%LOCALAPPDATA%\NiceHCK Controller\logs\`。
