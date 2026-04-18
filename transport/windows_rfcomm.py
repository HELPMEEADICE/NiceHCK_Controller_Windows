from __future__ import annotations

import asyncio
import threading
from typing import Callable, Optional
from uuid import UUID

from .base import BluetoothTransport
from ..util.device_match import BluetoothDeviceCandidate

SERVICE_UUID = UUID("0000a100-1000-8000-4e48-434b4354524c")


class WindowsRfcommTransport(BluetoothTransport):
    def __init__(self, candidate: BluetoothDeviceCandidate, timeout: float = 3.0) -> None:
        self.candidate = candidate
        self.timeout = timeout
        self._socket = None
        self._reader = None
        self._writer = None
        self._reader_thread: Optional[threading.Thread] = None
        self._stop_event = threading.Event()

    def connect(self) -> None:
        asyncio.run(self._connect_async())
        self._stop_event.clear()

    async def _connect_async(self) -> None:
        try:
            from winsdk.windows.devices.bluetooth import BluetoothCacheMode, BluetoothDevice
            from winsdk.windows.devices.bluetooth.rfcomm import RfcommServiceId
            from winsdk.windows.networking.sockets import StreamSocket
            from winsdk.windows.storage.streams import DataReader, DataWriter, InputStreamOptions
        except ImportError as exc:
            raise RuntimeError("缺少 winsdk，请先安装 requirements.txt 中的依赖") from exc

        device = await BluetoothDevice.from_id_async(self.candidate.device_id)
        if device is None:
            raise RuntimeError(f"无法打开蓝牙设备: {self.candidate.device_name}")

        service_id = RfcommServiceId.from_uuid(SERVICE_UUID)
        result = await device.get_rfcomm_services_for_id_async(service_id, BluetoothCacheMode.UNCACHED)
        services = list(result.services)
        if not services:
            raise RuntimeError(f"设备未暴露目标 RFCOMM 服务: {self.candidate.device_name}")

        service = services[0]
        await service.request_access_async()

        sock = StreamSocket()
        await sock.connect_async(service.connection_host_name, service.connection_service_name, service.protection_level)

        reader = DataReader(sock.input_stream)
        reader.input_stream_options = InputStreamOptions.PARTIAL
        writer = DataWriter(sock.output_stream)

        self._socket = sock
        self._reader = reader
        self._writer = writer
        self.candidate.service_name = service.connection_service_name

    def disconnect(self) -> None:
        self._stop_event.set()
        if self._reader_thread and self._reader_thread.is_alive():
            self._reader_thread.join(timeout=1.0)
        self._reader_thread = None
        self._reader = None
        self._writer = None
        if self._socket is not None:
            self._socket.close()
            self._socket = None

    def send(self, data: bytes) -> None:
        if self._writer is None:
            raise RuntimeError("RFCOMM 未连接")
        asyncio.run(self._send_async(data))

    async def _send_async(self, data: bytes) -> None:
        self._writer.write_bytes(data)
        await self._writer.store_async()
        await self._writer.flush_async()

    def start_reader(self, on_data: Callable[[bytes], None], on_error: Callable[[Exception], None]) -> None:
        if self._reader is None:
            raise RuntimeError("RFCOMM 未连接")
        if self._reader_thread and self._reader_thread.is_alive():
            return

        def _reader_loop() -> None:
            try:
                while not self._stop_event.is_set() and self._reader is not None:
                    chunk = asyncio.run(self._read_once_async())
                    if chunk:
                        on_data(chunk)
            except Exception as exc:  # pragma: no cover - runtime IO path
                if not self._stop_event.is_set():
                    on_error(exc)

        self._reader_thread = threading.Thread(target=_reader_loop, name="nicehck-rfcomm-reader", daemon=True)
        self._reader_thread.start()

    async def _read_once_async(self) -> bytes:
        loaded = await self._reader.load_async(512)
        if loaded == 0:
            return b""
        buffer = bytearray(loaded)
        self._reader.read_bytes(buffer)
        return bytes(buffer)

    def is_connected(self) -> bool:
        return self._socket is not None
