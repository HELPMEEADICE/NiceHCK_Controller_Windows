# NiceHCK Desktop Controller

一个给 Windows 用的 NiceHCK 蓝牙耳机桌面控制器，界面用 Tkinter，底层同时兼容 RFCOMM 和串口 SPP 两种连接路径。它干的事很直接：自动找设备、建立连接、读取耳机状态，然后把 ANC、EQ、编码、游戏模式和低延迟这些控制项摆到桌面上，不用再和一堆玄学操作较劲。

## 功能

- 自动发现并连接 NiceHCK 相关蓝牙设备
- 优先使用 Windows RFCOMM 服务，找不到时回退到蓝牙串口 SPP
- 查询并展示连接状态、设备名称、端口、固件版本和电量信息
- 切换 ANC 模式、EQ 模式、蓝牙编码、游戏模式、低延迟模式
- 显示收发报文和状态变化日志，方便排查协议行为
- 包含协议层单元测试，验证指令封包和解析逻辑

## 项目结构

```text
.
├─app.py                     # Tkinter 应用入口
├─main.py                    # 启动脚本
├─core/
│  ├─controller.py           # 控制流程、状态同步、异步发送
│  ├─models.py               # 状态模型与枚举定义
│  └─protocol.py             # 协议封包、解析器、查询指令
├─transport/
│  ├─base.py                 # 传输抽象接口
│  ├─windows_rfcomm.py       # Windows RFCOMM 实现
│  └─windows_serial_spp.py   # Windows 串口 SPP 实现
├─ui/
│  └─main_window.py          # 桌面界面
├─util/
│  ├─device_match.py         # 设备与端口发现逻辑
│  └─logging.py              # 日志配置
└─tests/
   └─test_protocol.py        # 协议测试
```

## 运行环境

- Windows 10 / 11
- Python 3.11 或更高版本
- 已与系统完成配对的 NiceHCK 蓝牙耳机

## 安装依赖

```bash
pip install -r requirements.txt
```

依赖包括：

- `pyserial`：用于串口 SPP 通信
- `winsdk`：用于访问 Windows 蓝牙 RFCOMM API
- `pytest`：用于运行测试

## 启动方式

```bash
python main.py
```

启动后可以在界面里直接点击“自动连接”。程序会先尝试走 RFCOMM，如果目标服务没露出来，再回退到系统里的蓝牙串口。连接成功后会自动查询固件、电量、ANC、EQ、游戏模式和低延迟状态。

## 支持的控制项

### ANC 模式

- 关闭
- 通透
- 普通降噪
- 深度降噪
- 试验性降噪
- 风噪抑制

### EQ 模式

- 悔恨之泪
- 均衡中正
- 欧美澎湃
- 真律还原
- 游戏优化
- 细腻佳音
- 温婉人声

### 编码模式

- AAC
- LHDC
- SBC

部分功能受固件版本限制。根据当前实现，较新的固件版本支持扩展 EQ 和完整编码切换；旧固件会限制某些选项，界面也会按能力自动收缩，不会硬着头皮把不支持的东西塞给设备。

## 日志

程序运行时会把发送、接收和状态变化写入日志区域，同时也会输出到本地日志文件：

```text
logs/desktop_gui.log
```

如果你想看协议到底发了什么字节，这地方就是第一现场，别去靠猜，那玩意儿通常只会把人带进沟里。

## 测试

```bash
pytest
```

当前测试覆盖了这些内容：

- 查询类指令封包是否正确
- 设置类指令封包是否正确
- 分包和粘包场景下解析器是否正常工作
- 设备状态解析是否符合预期
- 启动查询序列是否包含关键状态项

## 工作原理概览

应用启动后创建 Tkinter 主窗口，由 `NiceHckDesktopController` 负责管理连接、发送协议命令和接收设备响应。收到的数据流会经过 `PacketStreamParser` 组包，再由 `parse_state_update()` 解析成设备状态，最后同步到 UI。整个流程不复杂，属于那种看起来朴素、但真少一层都容易开始发疯的结构。

## 注意事项

1. 使用前请先在 Windows 蓝牙设置中完成耳机配对。
2. 如果 RFCOMM 无法建立连接，程序会尝试通过蓝牙串口连接，这通常意味着系统暴露的是另一套入口。
3. 如果缺少 `winsdk` 或 `pyserial`，连接阶段会直接报错，不会假装自己还能硬撑。
4. 某些模式切换依赖固件版本，旧版本设备可能不支持完整能力。