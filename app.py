from __future__ import annotations

import tkinter as tk

from .ui.main_window import MainWindow


def run() -> None:
    root = tk.Tk()
    MainWindow(root)
    root.mainloop()
