from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass
from typing import Iterable, List, Optional


DEFAULT_PATTERNS = ("YUANDAO", "OriG", "NiceHCK", "Controller")


@dataclass(slots=True)
class CandidatePort:
    device_name: str
    port_name: str
    description: str


@dataclass(slots=True)
class BluetoothDeviceCandidate:
    device_name: str
    device_id: str
    address: str
    service_name: str
    description: str


def discover_candidate_ports(patterns: Iterable[str] = DEFAULT_PATTERNS) -> List[CandidatePort]:
    try:
        from serial.tools import list_ports
    except ImportError as exc:
        raise RuntimeError("缺少 pyserial，请先安装 requirements.txt 中的依赖") from exc

    ports = list(list_ports.comports())
    normalized = tuple(pattern.lower() for pattern in patterns)
    candidates: List[CandidatePort] = []
    seen_ports: set[str] = set()

    for port in ports:
        description = port.description or ""
        hwid = port.hwid or ""
        text = " ".join(filter(None, [port.device, port.name, description, hwid])).lower()
        if any(pattern in text for pattern in normalized):
            seen_ports.add(port.device)
            candidates.append(CandidatePort(
                device_name=_extract_name(description) or port.name or port.device,
                port_name=port.device,
                description=description or hwid or port.device,
            ))

    paired_names = _list_paired_bluetooth_devices()
    if paired_names:
        matched_names = [name for name in paired_names if any(pattern in name.lower() for pattern in normalized)] or paired_names
        bluetooth_ports = [port for port in ports if _looks_like_bluetooth_port(port.description, port.hwid)]
        for port in bluetooth_ports:
            if port.device in seen_ports:
                continue
            device_name = matched_names[0] if matched_names else (_extract_name(port.description or "") or port.device)
            candidates.append(CandidatePort(
                device_name=device_name,
                port_name=port.device,
                description=port.description or port.hwid or port.device,
            ))
            seen_ports.add(port.device)

    if candidates:
        return candidates

    for port in ports:
        if port.device in seen_ports:
            continue
        candidates.append(CandidatePort(
            device_name=_extract_name(port.description or "") or port.name or port.device,
            port_name=port.device,
            description=port.description or port.hwid or port.device,
        ))

    return candidates


def choose_best_port(patterns: Iterable[str] = DEFAULT_PATTERNS) -> Optional[CandidatePort]:
    candidates = discover_candidate_ports(patterns)
    if not candidates:
        return None
    candidates.sort(key=lambda candidate: (0 if _looks_like_bluetooth_port(candidate.description, candidate.description) else 1, candidate.port_name))
    return candidates[0]


def discover_rfcomm_devices(patterns: Iterable[str] = DEFAULT_PATTERNS, uuid: str = "0000a100-1000-8000-4e48-434b4354524c") -> List[BluetoothDeviceCandidate]:
    try:
        import asyncio
        from winsdk.windows.devices.bluetooth import BluetoothDevice
        from winsdk.windows.devices.enumeration import DeviceInformation
    except ImportError as exc:
        raise RuntimeError("缺少 winsdk，请先安装 requirements.txt 中的依赖") from exc

    async def _discover() -> List[BluetoothDeviceCandidate]:
        normalized = tuple(pattern.lower() for pattern in patterns)
        selector = BluetoothDevice.get_device_selector_from_pairing_state(True)
        infos = await DeviceInformation.find_all_async(selector, [])
        candidates: List[BluetoothDeviceCandidate] = []
        for info in infos:
            name = info.name or "未知蓝牙设备"
            if normalized and not any(pattern in name.lower() for pattern in normalized):
                continue
            address = _extract_address_from_device_id(info.id)
            candidates.append(BluetoothDeviceCandidate(
                device_name=name,
                device_id=info.id,
                address=address,
                service_name="",
                description=uuid,
            ))
        return candidates

    return asyncio.run(_discover())


def _extract_name(description: str) -> Optional[str]:
    match = re.search(r"Standard Serial over Bluetooth link \((.+?)\)", description)
    if match:
        return match.group(1)
    return description.strip() or None


def _extract_address_from_device_id(device_id: str) -> str:
    match = re.search(r"([0-9A-F]{12})", device_id, re.IGNORECASE)
    if not match:
        return device_id
    raw = match.group(1).upper()
    return ":".join(raw[index:index + 2] for index in range(0, len(raw), 2))


def _looks_like_bluetooth_port(description: Optional[str], hwid: Optional[str]) -> bool:
    text = " ".join(filter(None, [description, hwid])).lower()
    return any(token in text for token in ("bluetooth", "bthmodem", "rfcomm", "standard serial over bluetooth"))


def describe_available_ports() -> List[str]:
    try:
        from serial.tools import list_ports
    except ImportError:
        return ["pyserial 未安装"]

    descriptions: List[str] = []
    for port in list_ports.comports():
        description = port.description or ""
        hwid = port.hwid or ""
        descriptions.append(f"{port.device} | {description or '-'} | {hwid or '-'}")
    return descriptions


def describe_available_rfcomm_devices(patterns: Iterable[str] = DEFAULT_PATTERNS) -> List[str]:
    try:
        candidates = discover_rfcomm_devices(patterns)
    except Exception as exc:
        return [str(exc)]
    return [f"{candidate.device_name} | {candidate.address} | ports={list(candidate.ports)} | {candidate.description}" for candidate in candidates]


def _list_paired_bluetooth_devices() -> List[str]:
    return [record["name"] for record in _list_paired_bluetooth_device_records() if record.get("name")]


def _list_paired_bluetooth_device_records() -> List[dict[str, str]]:
    script = """
$devices = Get-PnpDevice -Class Bluetooth | Where-Object { $_.Status -ne $null }
foreach ($d in $devices) {
  $instance = $d.InstanceId
  $name = $d.FriendlyName
  $address = $null
  if ($instance -match '([0-9A-F]{12})') {
    $raw = $Matches[1]
    $address = ($raw -split '(..)' | Where-Object { $_ }) -join ':'
  }
  [PSCustomObject]@{
    Name = $name
    Address = $address
    Description = $instance
  }
} | ConvertTo-Json -Compress
"""
    try:
        completed = subprocess.run(
            ["powershell", "-NoProfile", "-Command", script],
            capture_output=True,
            text=True,
            check=False,
            timeout=15,
        )
    except Exception:
        return []

    if completed.returncode != 0 or not completed.stdout.strip():
        return []

    try:
        import json
        payload = json.loads(completed.stdout)
    except Exception:
        return []

    if isinstance(payload, dict):
        payload = [payload]

    results: List[dict[str, str]] = []
    for item in payload:
        if not isinstance(item, dict):
            continue
        name = str(item.get("Name") or "").strip()
        address = str(item.get("Address") or "").strip()
        description = str(item.get("Description") or "").strip()
        if not name:
            continue
        results.append({"name": name, "address": address, "description": description})
    return results
