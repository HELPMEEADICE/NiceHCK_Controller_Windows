from __future__ import annotations

import importlib
import sys
from pathlib import Path

if __package__ in {None, ""}:
    package_root = Path(__file__).resolve().parent
    sys.path.insert(0, str(package_root.parent))
    run = importlib.import_module(f"{package_root.name}.app").run
else:
    from .app import run


if __name__ == "__main__":
    run()
