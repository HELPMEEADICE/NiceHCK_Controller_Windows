from __future__ import annotations

import threading
from typing import Callable, Optional

from .base import BluetoothTransport
from ..util.device_match import CandidatePort


class WindowsSerialSppTransport(BluetoothTransport):
    def __init__(self, candidate: CandidatePort, baudrate: int = 115200, timeout: float = 0.2) -> None:
        self.candidate = candidate
        self.baudrate = baudrate
        self.timeout = timeout
        self._serial = None
        self._reader_thread: Optional[threading.Thread] = None
        self._stop_event = threading.Event()

    def connect(self) -> None:
        try:
            import serial
        except ImportError as exc:
            raise RuntimeError("缺少 pyserial，请先安装 requirements.txt 中的依赖") from exc

        self._serial = serial.Serial(self.candidate.port_name, self.baudrate, timeout=self.timeout)
        self._stop_event.clear()

    def disconnect(self) -> None:
        self._stop_event.set()
        if self._reader_thread and self._reader_thread.is_alive():
            self._reader_thread.join(timeout=1.0)
        self._reader_thread = None

        if self._serial is not None:
            self._serial.close()
            self._serial = None

    def send(self, data: bytes) -> None:
        if not self._serial:
            raise RuntimeError("蓝牙串口未连接")
        self._serial.write(data)
        self._serial.flush()

    def start_reader(self, on_data: Callable[[bytes], None], on_error: Callable[[Exception], None]) -> None:
        if not self._serial:
            raise RuntimeError("蓝牙串口未连接")
        if self._reader_thread and self._reader_thread.is_alive():
            return

        def _reader() -> None:
            try:
                while not self._stop_event.is_set() and self._serial is not None:
                    chunk = self._serial.read(512)
                    if chunk:
                        on_data(chunk)
            except Exception as exc:  # pragma: no cover - runtime IO path
                if not self._stop_event.is_set():
                    on_error(exc)

        self._reader_thread = threading.Thread(target=_reader, name="nicehck-spp-reader", daemon=True)
        self._reader_thread.start()

    def is_connected(self) -> bool:
        return self._serial is not None and bool(getattr(self._serial, "is_open", False))
