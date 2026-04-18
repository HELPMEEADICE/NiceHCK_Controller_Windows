from __future__ import annotations

import queue
import threading
import time
from typing import Any, Callable, Optional

from .models import AncMode, CodecMode, ConnectionState, DeviceState, EqMode
from .protocol import (
    PacketStreamParser,
    parse_state_update,
    paced_startup_queries,
    query_anc,
    query_eq,
    query_game_mode,
    query_low_latency,
    set_anc,
    set_codec,
    set_eq,
    set_game_mode,
    set_legacy_codec_lhdc,
    set_low_latency,
)
from ..transport.base import BluetoothTransport
from ..transport.windows_rfcomm import WindowsRfcommTransport
from ..transport.windows_serial_spp import WindowsSerialSppTransport
from ..util.device_match import (
    BluetoothDeviceCandidate,
    CandidatePort,
    choose_best_port,
    describe_available_ports,
    describe_available_rfcomm_devices,
    discover_rfcomm_devices,
)
from ..util.logging import configure_logging


class NiceHckDesktopController:
    def __init__(self, event_callback: Callable[[str, dict[str, Any]], None]) -> None:
        self._event_callback = event_callback
        self._state = DeviceState()
        self._parser = PacketStreamParser()
        self._transport: Optional[BluetoothTransport] = None
        self._worker_lock = threading.Lock()
        self._write_lock = threading.Lock()
        self._logger = configure_logging()

    @property
    def state(self) -> DeviceState:
        return self._state

    def auto_connect(self) -> None:
        self._run_async(self._auto_connect)

    def refresh_state(self) -> None:
        self._run_async(self._refresh_state)

    def disconnect(self) -> None:
        self._run_async(self._disconnect)

    def set_anc_mode(self, mode: AncMode) -> None:
        self._run_async(self._set_anc_mode, mode)

    def set_eq_mode(self, mode: EqMode) -> None:
        self._run_async(self._set_eq_mode, mode)

    def set_codec_mode(self, mode: CodecMode) -> None:
        self._run_async(self._set_codec_mode, mode)

    def set_game_mode_enabled(self, enabled: bool) -> None:
        self._run_async(self._set_game_mode_enabled, enabled)

    def set_low_latency_enabled(self, enabled: bool) -> None:
        self._run_async(self._set_low_latency_enabled, enabled)

    def _run_async(self, func: Callable[..., None], *args: Any) -> None:
        threading.Thread(target=lambda: func(*args), daemon=True).start()

    def _auto_connect(self) -> None:
        with self._worker_lock:
            try:
                self._update_state(connection_state=ConnectionState.CONNECTING, last_error=None)

                rfcomm_candidates = discover_rfcomm_devices()
                if rfcomm_candidates:
                    rfcomm_candidate = rfcomm_candidates[0]
                    self._transport = WindowsRfcommTransport(rfcomm_candidate)
                    self._transport.connect()
                    self._transport.start_reader(self._handle_data, self._handle_error)
                    service_name = rfcomm_candidate.service_name or "?"
                    self._update_state(
                        connection_state=ConnectionState.CONNECTED,
                        device_name=rfcomm_candidate.device_name,
                        port_name=f"RFCOMM:{rfcomm_candidate.address}:{service_name}",
                    )
                    self._log(
                        f"已通过 RFCOMM 连接 {rfcomm_candidate.device_name} @ "
                        f"{rfcomm_candidate.address}:{service_name} ({rfcomm_candidate.description})"
                    )
                    self._send_startup_queries()
                    return

                candidate = choose_best_port()
                if candidate is None:
                    rfcomm_lines = describe_available_rfcomm_devices()
                    port_lines = describe_available_ports()
                    detail_parts = []
                    if rfcomm_lines:
                        detail_parts.append("RFCOMM=" + "；".join(rfcomm_lines))
                    if port_lines:
                        detail_parts.append("PORT=" + "；".join(port_lines))
                    detail = " | ".join(detail_parts) if detail_parts else "未检测到任何 RFCOMM 服务或串口"
                    raise RuntimeError(f"未找到可用蓝牙设备。当前检测结果：{detail}")

                self._transport = WindowsSerialSppTransport(candidate)
                self._transport.connect()
                self._transport.start_reader(self._handle_data, self._handle_error)

                self._update_state(
                    connection_state=ConnectionState.CONNECTED,
                    device_name=candidate.device_name,
                    port_name=candidate.port_name,
                )
                self._log(f"已通过串口连接 {candidate.device_name} @ {candidate.port_name} ({candidate.description})")
                self._send_startup_queries()
            except Exception as exc:
                self._handle_error(exc)

    def _disconnect(self) -> None:
        with self._worker_lock:
            if self._transport is not None:
                self._transport.disconnect()
                self._transport = None
            self._parser = PacketStreamParser()
            self._update_state(
                connection_state=ConnectionState.DISCONNECTED,
                device_name="未连接",
                port_name=None,
                left_battery=None,
                right_battery=None,
                case_battery=None,
                selected_codec=None,
            )
            self._log("已断开连接")

    def _refresh_state(self) -> None:
        with self._worker_lock:
            self._ensure_connected()
            self._send_startup_queries()
            self._log("已发送状态查询")

    def _set_anc_mode(self, mode: AncMode) -> None:
        with self._worker_lock:
            self._ensure_connected()
            self._send(set_anc(mode))
            time.sleep(0.1)
            self._send(query_anc())
            self._log(f"已发送 ANC 切换: {mode.label}")

    def _set_eq_mode(self, mode: EqMode) -> None:
        with self._worker_lock:
            self._ensure_connected()
            if not self._state.firmware.supports_extended_eq and mode in {EqMode.FINE, EqMode.VOCAL}:
                raise RuntimeError("当前固件版本过低，不支持该 EQ 模式")
            self._send(set_eq(mode))
            time.sleep(0.1)
            self._send(query_eq())
            self._log(f"已发送 EQ 切换: {mode.label}")

    def _set_codec_mode(self, mode: CodecMode) -> None:
        with self._worker_lock:
            self._ensure_connected()
            if self._state.firmware.supports_modern_codec_switch:
                self._send(set_codec(mode))
            else:
                if mode == CodecMode.SBC:
                    raise RuntimeError("当前固件版本过低，不支持 SBC 编码切换")
                self._send(set_legacy_codec_lhdc(mode == CodecMode.LHDC))
            self._update_state(selected_codec=mode)
            self._log(f"已发送编码切换: {mode.label}")

    def _set_game_mode_enabled(self, enabled: bool) -> None:
        with self._worker_lock:
            self._ensure_connected()
            self._send(set_game_mode(enabled))
            time.sleep(0.1)
            self._send(query_game_mode())
            self._log(f"已发送游戏模式切换: {'开' if enabled else '关'}")

    def _set_low_latency_enabled(self, enabled: bool) -> None:
        with self._worker_lock:
            self._ensure_connected()
            self._send(set_low_latency(enabled))
            time.sleep(0.1)
            self._send(query_low_latency())
            self._log(f"已发送低延迟切换: {'开' if enabled else '关'}")

    def _send_startup_queries(self) -> None:
        for packet in paced_startup_queries():
            self._send(packet)
            time.sleep(0.1)

    def _ensure_connected(self) -> None:
        if self._transport is None or not self._transport.is_connected():
            raise RuntimeError("当前未连接耳机")

    def _send(self, packet: bytes) -> None:
        if self._transport is None:
            raise RuntimeError("蓝牙传输未初始化")
        with self._write_lock:
            self._transport.send(packet)
            self._log(f"发送: {packet.hex(' ').upper()}")

    def _handle_data(self, data: bytes) -> None:
        self._log(f"接收: {data.hex(' ').upper()}")
        for message in self._parser.feed(data):
            update = parse_state_update(message)
            updates: dict[str, Any] = {}
            if update.firmware is not None:
                updates["firmware"] = update.firmware
            if update.left_battery is not None:
                updates["left_battery"] = update.left_battery
            if update.right_battery is not None:
                updates["right_battery"] = update.right_battery
            if update.case_battery is not None:
                updates["case_battery"] = update.case_battery
            if update.anc_mode is not None:
                updates["anc_mode"] = update.anc_mode
            if update.eq_mode is not None:
                updates["eq_mode"] = update.eq_mode
            if update.game_mode_enabled is not None:
                updates["game_mode_enabled"] = update.game_mode_enabled
            if update.low_latency_enabled is not None:
                updates["low_latency_enabled"] = update.low_latency_enabled
            if updates:
                self._log("状态更新: " + ", ".join(f"{key}={value}" for key, value in updates.items()))
                self._update_state(**updates)

    def _handle_error(self, exc: Exception) -> None:
        self._logger.exception("桌面控制器错误", exc_info=exc)
        self._update_state(connection_state=ConnectionState.ERROR, last_error=str(exc))
        self._emit("error", {"message": str(exc)})

    def _update_state(self, **changes: Any) -> None:
        for key, value in changes.items():
            setattr(self._state, key, value)
        self._emit("state", {"state": self._state})

    def _log(self, message: str) -> None:
        self._logger.info(message)
        self._emit("log", {"message": message})

    def _emit(self, event_type: str, payload: dict[str, Any]) -> None:
        self._event_callback(event_type, payload)
