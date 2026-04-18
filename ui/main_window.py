from __future__ import annotations

import queue
import tkinter as tk
from tkinter import messagebox, ttk
from typing import Any

from ..core.controller import NiceHckDesktopController
from ..core.models import AncMode, CodecMode, ConnectionState, DeviceState, EqMode


class MainWindow(ttk.Frame):
    def __init__(self, master: tk.Tk) -> None:
        super().__init__(master, padding=16)
        self.master = master
        self.event_queue: queue.Queue[tuple[str, dict[str, Any]]] = queue.Queue()
        self.controller = NiceHckDesktopController(self._enqueue_event)

        self.connection_var = tk.StringVar(value="未连接")
        self.device_var = tk.StringVar(value="设备：未连接")
        self.port_var = tk.StringVar(value="串口：-")
        self.firmware_var = tk.StringVar(value="固件：未知")
        self.battery_var = tk.StringVar(value="电量：左 - | 右 - | 盒 -")
        self.codec_status_var = tk.StringVar(value="上次发送编码：-")
        self.game_mode_var = tk.BooleanVar(value=False)
        self.low_latency_var = tk.BooleanVar(value=False)
        self.anc_var = tk.StringVar(value=AncMode.OFF.name)
        self.eq_var = tk.StringVar(value=EqMode.BALANCED.name)

        self.log_text = tk.Text(width=72, height=14, state="disabled", font=("DengXian", 10))

        self._build_ui()
        self.grid(sticky="nsew")
        self.master.after(100, self._drain_events)

    def _build_ui(self) -> None:
        self.master.title("NiceHCK Desktop Controller")
        self.master.minsize(760, 560)
        self.master.columnconfigure(0, weight=1)
        self.master.rowconfigure(0, weight=1)
        self.columnconfigure(0, weight=1)

        header = ttk.Label(self, text="NiceHCK 桌面控制器", font=("Microsoft YaHei UI", 16, "bold"))
        header.grid(row=0, column=0, sticky="w")

        status_frame = ttk.LabelFrame(self, text="连接状态", padding=12)
        status_frame.grid(row=1, column=0, sticky="ew", pady=(12, 8))
        status_frame.columnconfigure(1, weight=1)

        ttk.Label(status_frame, text="状态：").grid(row=0, column=0, sticky="w")
        ttk.Label(status_frame, textvariable=self.connection_var).grid(row=0, column=1, sticky="w")
        ttk.Label(status_frame, textvariable=self.device_var).grid(row=1, column=0, columnspan=2, sticky="w", pady=(6, 0))
        ttk.Label(status_frame, textvariable=self.port_var).grid(row=2, column=0, columnspan=2, sticky="w", pady=(6, 0))
        ttk.Label(status_frame, textvariable=self.firmware_var).grid(row=3, column=0, columnspan=2, sticky="w", pady=(6, 0))
        ttk.Label(status_frame, textvariable=self.battery_var).grid(row=4, column=0, columnspan=2, sticky="w", pady=(6, 0))
        ttk.Label(status_frame, textvariable=self.codec_status_var).grid(row=5, column=0, columnspan=2, sticky="w", pady=(6, 0))

        button_row = ttk.Frame(status_frame)
        button_row.grid(row=6, column=0, columnspan=2, sticky="w", pady=(12, 0))
        ttk.Button(button_row, text="自动连接", command=self.controller.auto_connect).grid(row=0, column=0, padx=(0, 8))
        ttk.Button(button_row, text="刷新状态", command=self.controller.refresh_state).grid(row=0, column=1, padx=(0, 8))
        ttk.Button(button_row, text="断开连接", command=self.controller.disconnect).grid(row=0, column=2)

        control_frame = ttk.LabelFrame(self, text="模式切换", padding=12)
        control_frame.grid(row=2, column=0, sticky="nsew", pady=(0, 8))
        control_frame.columnconfigure(1, weight=1)
        self.rowconfigure(2, weight=1)

        anc_values = [mode.name for mode in AncMode]
        self.anc_labels = {mode.name: mode.label for mode in AncMode}
        self.anc_reverse_labels = {value: key for key, value in self.anc_labels.items()}
        ttk.Label(control_frame, text="降噪模式").grid(row=0, column=0, sticky="w")
        self.anc_combo = ttk.Combobox(control_frame, state="readonly", values=[self.anc_labels[value] for value in anc_values])
        self.anc_combo.grid(row=0, column=1, sticky="ew", padx=(12, 8))
        self.anc_combo.current(0)
        ttk.Button(control_frame, text="应用 ANC", command=lambda: self._apply_anc(self.anc_combo.get())).grid(row=0, column=2)

        eq_values = [mode.name for mode in EqMode]
        self.eq_labels = {mode.name: mode.label for mode in EqMode}
        self.eq_reverse_labels = {value: key for key, value in self.eq_labels.items()}
        ttk.Label(control_frame, text="均衡器").grid(row=1, column=0, sticky="w", pady=(12, 0))
        self.eq_combo = ttk.Combobox(control_frame, state="readonly", values=[self.eq_labels[value] for value in eq_values])
        self.eq_combo.grid(row=1, column=1, sticky="ew", padx=(12, 8), pady=(12, 0))
        self.eq_combo.current(eq_values.index(EqMode.BALANCED.name))
        ttk.Button(control_frame, text="应用 EQ", command=lambda: self._apply_eq(self.eq_combo.get())).grid(row=1, column=2, pady=(12, 0))

        codec_values = [mode.name for mode in CodecMode]
        self.codec_labels = {mode.name: mode.label for mode in CodecMode}
        self.codec_reverse_labels = {value: key for key, value in self.codec_labels.items()}
        ttk.Label(control_frame, text="蓝牙编码").grid(row=2, column=0, sticky="w", pady=(12, 0))
        self.codec_combo = ttk.Combobox(control_frame, state="disabled", values=[])
        self.codec_combo.grid(row=2, column=1, sticky="ew", padx=(12, 8), pady=(12, 0))
        self.codec_combo.set(self.codec_labels[CodecMode.AAC.name])
        ttk.Button(control_frame, text="应用编码", command=lambda: self._apply_codec(self.codec_combo.get())).grid(row=2, column=2, pady=(12, 0))
        self._codec_all_values = [self.codec_labels[value] for value in codec_values]
        self._codec_legacy_values = [self.codec_labels[CodecMode.AAC.name], self.codec_labels[CodecMode.LHDC.name]]

        game_mode_check = ttk.Checkbutton(
            control_frame,
            text="游戏模式",
            variable=self.game_mode_var,
            command=lambda: self.controller.set_game_mode_enabled(self.game_mode_var.get()),
        )
        game_mode_check.grid(row=3, column=0, columnspan=2, sticky="w", pady=(16, 0))

        low_latency_check = ttk.Checkbutton(
            control_frame,
            text="低延迟模式",
            variable=self.low_latency_var,
            command=lambda: self.controller.set_low_latency_enabled(self.low_latency_var.get()),
        )
        low_latency_check.grid(row=4, column=0, columnspan=2, sticky="w", pady=(8, 0))

        log_frame = ttk.LabelFrame(self, text="日志", padding=12)
        log_frame.grid(row=3, column=0, sticky="nsew")
        log_frame.columnconfigure(0, weight=1)
        log_frame.rowconfigure(0, weight=1)
        self.rowconfigure(3, weight=1)

        self.log_text.grid(in_=log_frame, row=0, column=0, sticky="nsew")
        scrollbar = ttk.Scrollbar(log_frame, orient="vertical", command=self.log_text.yview)
        scrollbar.grid(row=0, column=1, sticky="ns")
        self.log_text.configure(yscrollcommand=scrollbar.set)

    def _apply_anc(self, label: str) -> None:
        self.controller.set_anc_mode(AncMode[self.anc_reverse_labels[label]])

    def _apply_eq(self, label: str) -> None:
        self.controller.set_eq_mode(EqMode[self.eq_reverse_labels[label]])

    def _apply_codec(self, label: str) -> None:
        if not label:
            return
        self.controller.set_codec_mode(CodecMode[self.codec_reverse_labels[label]])

    def _enqueue_event(self, event_type: str, payload: dict[str, Any]) -> None:
        self.event_queue.put((event_type, payload))

    def _drain_events(self) -> None:
        while True:
            try:
                event_type, payload = self.event_queue.get_nowait()
            except queue.Empty:
                break
            if event_type == "state":
                self._apply_state(payload["state"])
            elif event_type == "log":
                self._append_log(payload["message"])
            elif event_type == "error":
                self._append_log(f"错误: {payload['message']}")
                messagebox.showerror("NiceHCK Desktop Controller", payload["message"])
        self.master.after(100, self._drain_events)

    def _apply_state(self, state: DeviceState) -> None:
        status_map = {
            ConnectionState.DISCONNECTED: "未连接",
            ConnectionState.CONNECTING: "连接中",
            ConnectionState.CONNECTED: "已连接",
            ConnectionState.ERROR: "错误",
        }
        self.connection_var.set(status_map[state.connection_state])
        self.device_var.set(f"设备：{state.device_name}")
        self.port_var.set(f"串口：{state.port_name or '-'}")
        self.firmware_var.set(f"固件：{state.firmware.display()}")
        self.battery_var.set(
            "电量：左 "
            f"{self._format_battery(state.left_battery)} | 右 {self._format_battery(state.right_battery)} | "
            f"盒 {self._format_battery(state.case_battery)}"
        )
        self.codec_status_var.set(f"上次发送编码：{state.selected_codec.label if state.selected_codec else '-'}")
        self._sync_codec_controls(state)

        anc_label = self.anc_labels.get(state.anc_mode.name)
        if anc_label and self.anc_combo.get() != anc_label:
            self.anc_combo.set(anc_label)

        eq_label = self.eq_labels.get(state.eq_mode.name)
        if eq_label and self.eq_combo.get() != eq_label:
            self.eq_combo.set(eq_label)

        if state.game_mode_enabled is not None:
            self.game_mode_var.set(state.game_mode_enabled)

        if state.low_latency_enabled is not None:
            self.low_latency_var.set(state.low_latency_enabled)

    def _sync_codec_controls(self, state: DeviceState) -> None:
        if not state.firmware.known:
            values = []
            combo_state = "disabled"
        elif state.firmware.supports_modern_codec_switch:
            values = self._codec_all_values
            combo_state = "readonly"
        else:
            values = self._codec_legacy_values
            combo_state = "readonly"

        self.codec_combo.configure(values=values, state=combo_state)

        preferred_label = state.selected_codec.label if state.selected_codec else None
        if preferred_label not in values:
            preferred_label = values[0] if values else ""
        if self.codec_combo.get() != preferred_label:
            self.codec_combo.set(preferred_label)

    @staticmethod
    def _format_battery(value: int | None) -> str:
        return f"{value}%" if value is not None else "-"

    def _append_log(self, message: str) -> None:
        self.log_text.configure(state="normal")
        self.log_text.insert("end", message + "\n")
        self.log_text.see("end")
        self.log_text.configure(state="disabled")
