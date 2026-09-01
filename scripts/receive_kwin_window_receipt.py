#!/usr/bin/env python3
"""Receive one private KWin-script window receipt and exit boundedly."""

from __future__ import annotations

import sys
from pathlib import Path

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib


SERVICE = "io.github.YellowWhiteBlackCat.TaskForest.CaptureReceipt"
PATH = "/Capture"
INTERFACE = "io.github.YellowWhiteBlackCat.CaptureReceipt"
TIMEOUT_MS = 8_000


class ReceiptReceiver(dbus.service.Object):
    def __init__(self, bus: dbus.Bus, output: Path, loop: GLib.MainLoop) -> None:
        super().__init__(bus, PATH)
        self.output = output
        self.loop = loop
        self.received = False

    @dbus.service.method(INTERFACE, in_signature="s", out_signature="")
    def publish(self, payload: str) -> None:
        if not payload.startswith("TASKFOREST_WINDOW "):
            return
        self.output.write_text(payload + "\n", encoding="utf-8")
        self.received = True
        GLib.idle_add(self.loop.quit)


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: receive_kwin_window_receipt.py OUTPUT READY", file=sys.stderr)
        return 2
    output = Path(sys.argv[1])
    ready = Path(sys.argv[2])
    DBusGMainLoop(set_as_default=True)
    loop = GLib.MainLoop()
    bus = dbus.SessionBus()
    name = dbus.service.BusName(SERVICE, bus=bus, do_not_queue=True)
    receiver = ReceiptReceiver(bus, output, loop)
    receiver.bus_name = name
    ready.touch()
    GLib.timeout_add(TIMEOUT_MS, loop.quit)
    loop.run()
    return 0 if receiver.received else 1


if __name__ == "__main__":
    raise SystemExit(main())
