from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Callable


class BluetoothTransport(ABC):
    @abstractmethod
    def connect(self) -> None:
        raise NotImplementedError

    @abstractmethod
    def disconnect(self) -> None:
        raise NotImplementedError

    @abstractmethod
    def send(self, data: bytes) -> None:
        raise NotImplementedError

    @abstractmethod
    def start_reader(self, on_data: Callable[[bytes], None], on_error: Callable[[Exception], None]) -> None:
        raise NotImplementedError

    @abstractmethod
    def is_connected(self) -> bool:
        raise NotImplementedError
