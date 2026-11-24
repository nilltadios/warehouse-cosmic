#!/usr/bin/env python3
"""Real X11 XDND drops into the dev Flatpak, on a private Xvfb display.

    flatpak/build.sh --dev
    python3 verification/check-x11-drop-flatpak.py

Installs flatpak/out-dev into a private FLATPAK_USER_DIR under $HOME (the
user's installation is shared read-only for runtimes), starts one isolated GUI
with --nosocket=wayland on its own Xvfb, and acts as an XDND drag source:
hover/leave toggle the drop banner, a .flatpak with a space in its name opens
install review without installing, a .flatpakrepo opens Add Remote, and
unsupported or mixed drops are refused. Closes its window and removes its
directory. Needs Xvfb and libX11; touches no other window or display.
"""
from __future__ import annotations

import ctypes
from ctypes import c_char_p, c_int, c_long, c_uint, c_ulong, c_void_p, POINTER, byref, Structure, Union
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from urllib.parse import quote


# Minimal Xlib structures for ClientMessage / SelectionRequest / SelectionNotify (64-bit Linux).
class XClientMessageData(Union):
    _fields_ = [
        ("b", ctypes.c_char * 20),
        ("s", ctypes.c_short * 10),
        ("l", c_long * 5),
    ]


class XClientMessageEvent(Structure):
    _fields_ = [
        ("type", c_int),
        ("serial", c_ulong),
        ("send_event", c_int),
        ("display", c_void_p),
        ("window", c_ulong),
        ("message_type", c_ulong),
        ("format", c_int),
        ("data", XClientMessageData),
    ]


class XSelectionRequestEvent(Structure):
    _fields_ = [
        ("type", c_int),
        ("serial", c_ulong),
        ("send_event", c_int),
        ("display", c_void_p),
        ("owner", c_ulong),
        ("requestor", c_ulong),
        ("selection", c_ulong),
        ("target", c_ulong),
        ("property", c_ulong),
        ("time", c_ulong),
    ]


class XSelectionEvent(Structure):
    _fields_ = [
        ("type", c_int),
        ("serial", c_ulong),
        ("send_event", c_int),
        ("display", c_void_p),
        ("requestor", c_ulong),
        ("selection", c_ulong),
        ("target", c_ulong),
        ("property", c_ulong),
        ("time", c_ulong),
    ]


class XEvent(Union):
    _fields_ = [
        ("type", c_int),
        ("xclient", XClientMessageEvent),
        ("xselectionrequest", XSelectionRequestEvent),
        ("xselection", XSelectionEvent),
        ("pad", c_long * 24),
    ]


CLIENT_MESSAGE = 33
SELECTION_REQUEST = 30
SELECTION_NOTIFY = 31
PROP_MODE_REPLACE = 0
CURRENT_TIME = 0


class X11Client:
    def __init__(self, display_name: str):
        lib = ctypes.CDLL("libX11.so.6")
        self.lib = lib

        lib.XOpenDisplay.argtypes = [c_char_p]
        lib.XOpenDisplay.restype = c_void_p
        lib.XCloseDisplay.argtypes = [c_void_p]
        lib.XCloseDisplay.restype = c_int
        lib.XDefaultRootWindow.argtypes = [c_void_p]
        lib.XDefaultRootWindow.restype = c_ulong
        lib.XCreateSimpleWindow.argtypes = [
            c_void_p, c_ulong, c_int, c_int, c_uint, c_uint, c_uint, c_ulong, c_ulong
        ]
        lib.XCreateSimpleWindow.restype = c_ulong
        lib.XDestroyWindow.argtypes = [c_void_p, c_ulong]
        lib.XDestroyWindow.restype = c_int
        lib.XInternAtom.argtypes = [c_void_p, c_char_p, c_int]
        lib.XInternAtom.restype = c_ulong
        lib.XQueryTree.argtypes = [
            c_void_p, c_ulong, POINTER(c_ulong), POINTER(c_ulong),
            POINTER(POINTER(c_ulong)), POINTER(c_uint),
        ]
        lib.XQueryTree.restype = c_int
        lib.XFree.argtypes = [c_void_p]
        lib.XFree.restype = c_int
        lib.XGetWindowProperty.argtypes = [
            c_void_p, c_ulong, c_ulong, c_long, c_long, c_int, c_ulong,
            POINTER(c_ulong), POINTER(c_int), POINTER(c_ulong), POINTER(c_ulong),
            POINTER(c_void_p),
        ]
        lib.XGetWindowProperty.restype = c_int
        lib.XSetSelectionOwner.argtypes = [c_void_p, c_ulong, c_ulong, c_ulong]
        lib.XSetSelectionOwner.restype = c_int
        lib.XGetSelectionOwner.argtypes = [c_void_p, c_ulong]
        lib.XGetSelectionOwner.restype = c_ulong
        lib.XChangeProperty.argtypes = [
            c_void_p, c_ulong, c_ulong, c_ulong, c_int, c_int, c_char_p, c_int
        ]
        lib.XChangeProperty.restype = c_int
        lib.XSendEvent.argtypes = [c_void_p, c_ulong, c_int, c_long, POINTER(XEvent)]
        lib.XSendEvent.restype = c_int
        lib.XFlush.argtypes = [c_void_p]
        lib.XFlush.restype = c_int
        lib.XPending.argtypes = [c_void_p]
        lib.XPending.restype = c_int
        lib.XNextEvent.argtypes = [c_void_p, POINTER(XEvent)]
        lib.XNextEvent.restype = c_int

        self.dpy = lib.XOpenDisplay(display_name.encode())
        if not self.dpy:
            raise RuntimeError(f"Cannot open X display {display_name}")
        self.root = lib.XDefaultRootWindow(self.dpy)
        self.src = lib.XCreateSimpleWindow(self.dpy, self.root, 0, 0, 10, 10, 0, 0, 0)

        self.xdnd_aware = self.atom("XdndAware")
        self.xdnd_enter = self.atom("XdndEnter")
        self.xdnd_position = self.atom("XdndPosition")
        self.xdnd_status = self.atom("XdndStatus")
        self.xdnd_drop = self.atom("XdndDrop")
        self.xdnd_leave = self.atom("XdndLeave")
        self.xdnd_finished = self.atom("XdndFinished")
        self.xdnd_selection = self.atom("XdndSelection")
        self.xdnd_action_copy = self.atom("XdndActionCopy")
        self.text_uri_list = self.atom("text/uri-list")

    def atom(self, name: str) -> int:
        return int(self.lib.XInternAtom(self.dpy, name.encode(), 0))

    def has_property(self, win: int, prop: int) -> bool:
        actual_type = c_ulong(0)
        actual_fmt = c_int(0)
        nitems = c_ulong(0)
        bytes_after = c_ulong(0)
        data = c_void_p(0)
        status = self.lib.XGetWindowProperty(
            self.dpy, win, prop, 0, 1, 0, 0,
            byref(actual_type), byref(actual_fmt), byref(nitems), byref(bytes_after), byref(data),
        )
        if data.value:
            self.lib.XFree(data)
        return status == 0 and actual_type.value != 0

    def find_xdnd_window(self) -> int | None:
        stack = [self.root]
        while stack:
            w = stack.pop()
            if w != self.root and w != self.src and self.has_property(w, self.xdnd_aware):
                return w
            r = c_ulong(0)
            p = c_ulong(0)
            children = POINTER(c_ulong)()
            n = c_uint(0)
            if self.lib.XQueryTree(self.dpy, w, byref(r), byref(p), byref(children), byref(n)):
                for i in range(n.value):
                    stack.append(int(children[i]))
                if children:
                    self.lib.XFree(children)
        return None

    def send_client_msg(self, target: int, msg_type: int, data_l: list[int]) -> None:
        ev = XEvent()
        ev.xclient.type = CLIENT_MESSAGE
        ev.xclient.window = target
        ev.xclient.message_type = msg_type
        ev.xclient.format = 32
        for i in range(5):
            ev.xclient.data.l[i] = data_l[i] if i < len(data_l) else 0
        self.lib.XSendEvent(self.dpy, target, 0, 0, byref(ev))
        self.lib.XFlush(self.dpy)

    def pump(self, payload: bytes, wait_for_msg: int | None, timeout: float = 5.0) -> XClientMessageEvent | None:
        deadline = time.monotonic() + timeout
        got: XClientMessageEvent | None = None
        while time.monotonic() < deadline:
            while self.lib.XPending(self.dpy) > 0:
                ev = XEvent()
                self.lib.XNextEvent(self.dpy, byref(ev))
                if ev.type == SELECTION_REQUEST:
                    req = ev.xselectionrequest
                    prop = req.property if req.property else req.target
                    reply = XEvent()
                    reply.xselection.type = SELECTION_NOTIFY
                    reply.xselection.display = req.display
                    reply.xselection.requestor = req.requestor
                    reply.xselection.selection = req.selection
                    reply.xselection.target = req.target
                    reply.xselection.time = req.time
                    if req.selection == self.xdnd_selection and req.target == self.text_uri_list:
                        self.lib.XChangeProperty(
                            self.dpy, req.requestor, prop, self.text_uri_list,
                            8, PROP_MODE_REPLACE, payload, len(payload),
                        )
                        reply.xselection.property = prop
                    else:
                        reply.xselection.property = 0
                    self.lib.XSendEvent(self.dpy, req.requestor, 0, 0, byref(reply))
                    self.lib.XFlush(self.dpy)
                elif ev.type == CLIENT_MESSAGE and wait_for_msg is not None:
                    if ev.xclient.message_type == wait_for_msg:
                        got = XClientMessageEvent()
                        ctypes.memmove(byref(got), byref(ev.xclient), ctypes.sizeof(XClientMessageEvent))
            if got is not None:
                # Give one short pass to answer any SelectionRequest queued right next to XdndStatus.
                time.sleep(0.05)
                while self.lib.XPending(self.dpy) > 0:
                    ev = XEvent()
                    self.lib.XNextEvent(self.dpy, byref(ev))
                    if ev.type == SELECTION_REQUEST:
                        req = ev.xselectionrequest
                        prop = req.property if req.property else req.target
                        reply = XEvent()
                        reply.xselection.type = SELECTION_NOTIFY
                        reply.xselection.display = req.display
                        reply.xselection.requestor = req.requestor
                        reply.xselection.selection = req.selection
                        reply.xselection.target = req.target
                        reply.xselection.time = req.time
                        if req.selection == self.xdnd_selection and req.target == self.text_uri_list:
                            self.lib.XChangeProperty(
                                self.dpy, req.requestor, prop, self.text_uri_list,
                                8, PROP_MODE_REPLACE, payload, len(payload),
                            )
                            reply.xselection.property = prop
                        else:
                            reply.xselection.property = 0
                        self.lib.XSendEvent(self.dpy, req.requestor, 0, 0, byref(reply))
                        self.lib.XFlush(self.dpy)
                return got
            time.sleep(0.02)
        return None

    def begin_drag(self, target: int, paths: list[Path]) -> bytes:
        payload = "".join(f"file://{quote(str(p))}\r\n" for p in paths).encode("utf-8")
        self.lib.XSetSelectionOwner(self.dpy, self.xdnd_selection, self.src, CURRENT_TIME)
        self.lib.XFlush(self.dpy)
        assert self.lib.XGetSelectionOwner(self.dpy, self.xdnd_selection) == self.src
        # XdndEnter: version 5 in high byte of l[1], single offered type text/uri-list in l[2].
        self.send_client_msg(target, self.xdnd_enter, [self.src, 5 << 24, self.text_uri_list, 0, 0])
        # XdndPosition at root (200, 200).
        self.send_client_msg(target, self.xdnd_position, [self.src, 0, (200 << 16) | 200, CURRENT_TIME, self.xdnd_action_copy])
        status = self.pump(payload, self.xdnd_status, timeout=5.0)
        assert status is not None, "Did not receive XdndStatus from target window"
        assert (status.data.l[1] & 1) == 1, f"Target rejected XDND drag: flags={status.data.l[1]}"
        return payload

    def leave_drag(self, target: int) -> None:
        self.send_client_msg(target, self.xdnd_leave, [self.src, 0, 0, 0, 0])
        time.sleep(0.15)

    def drop_drag(self, target: int, payload: bytes) -> None:
        self.send_client_msg(target, self.xdnd_drop, [self.src, 0, CURRENT_TIME, 0, 0])
        finished = self.pump(payload, self.xdnd_finished, timeout=5.0)
        assert finished is not None, "Did not receive XdndFinished from target window"
        assert (finished.data.l[1] & 1) == 1, f"Target reported rejected XdndFinished: flags={finished.data.l[1]}"

    def close(self) -> None:
        self.lib.XDestroyWindow(self.dpy, self.src)
        self.lib.XCloseDisplay(self.dpy)


def pick_display() -> str:
    for n in range(90, 120):
        if not Path(f"/tmp/.X11-unix/X{n}").exists():
            return f":{n}"
    raise RuntimeError("No free Xvfb display number")


def ui_cli(env: dict[str, str], instance: str, *args: str) -> dict:
    cmd = [
        "flatpak", "run", "--user", "--command=warehouse-cosmic-cli",
        "dev.nilltadios.WarehouseCosmic", "ui", "--instance", instance, *args,
    ]
    proc = subprocess.run(cmd, env=env, capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(f"CLI failed ({proc.returncode}): {proc.stdout}\n{proc.stderr}")
    return json.loads(proc.stdout)


def wait_state(env: dict[str, str], instance: str, pred, desc: str, timeout: float = 5.0) -> dict:
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            res = ui_cli(env, instance, "inspect")
            last = res["data"]["state"]
            if pred(last):
                return last
        except Exception as e:
            last = str(e)
        time.sleep(0.1)
    raise AssertionError(f"Timed out waiting for {desc}; last state: {last}")


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    flatpak_repo = repo_root / "flatpak/out-dev/repo"
    assert flatpak_repo.is_dir(), f"Missing {flatpak_repo}; run flatpak/build.sh --dev first"

    # Must live under $HOME so the Flatpak sandbox (--filesystem=home) sees fixtures and config.
    workdir = Path(tempfile.mkdtemp(prefix=".warehouse-x11-xdnd-", dir=Path.home()))
    display = pick_display()
    xvfb = None
    app_proc = None
    x11 = None
    env = os.environ.copy()
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("XAUTHORITY", None)
    env["DISPLAY"] = display
    env["XDG_SESSION_TYPE"] = "x11"
    env["FLATPAK_USER_DIR"] = str(workdir / "installation")
    env["FLATPAK_SYSTEM_DIR"] = str(Path.home() / ".local/share/flatpak")

    try:
        (workdir / "config").mkdir(parents=True)
        cosmic_cfg = Path.home() / ".config/cosmic"
        if cosmic_cfg.exists():
            (workdir / "config/cosmic").symlink_to(cosmic_cfg)

        # Install dev Flatpak into private FLATPAK_USER_DIR only.
        subprocess.run(
            ["flatpak", "remote-add", "--user", "--no-gpg-verify", "warehouse-cosmic-dev", f"file://{flatpak_repo}"],
            env=env, check=True, capture_output=True, text=True,
        )
        subprocess.run(
            ["flatpak", "install", "--user", "--no-deps", "--noninteractive", "-y", "--reinstall",
             "warehouse-cosmic-dev", "dev.nilltadios.WarehouseCosmic"],
            env=env, check=True, capture_output=True, text=True,
        )

        # Fixture files inside $HOME.
        bundle = workdir / "Example App.flatpak"
        bundle.write_bytes(b"placeholder bundle bytes\n")
        note = workdir / "notes.txt"
        note.write_text("not a flatpak\n")
        repo_file = workdir / "demo-remote.flatpakrepo"
        repo_file.write_text("[Flatpak Repo]\nUrl=https://example.invalid/repo\n")

        # Start isolated Xvfb (no host desktop disturbance).
        xvfb = subprocess.Popen(
            ["Xvfb", display, "-screen", "0", "1280x800x24", "-nolisten", "tcp"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        for _ in range(50):
            if Path(f"/tmp/.X11-unix/X{display[1:]}").exists():
                break
            time.sleep(0.05)

        instance = f"dev.nilltadios.WarehouseCosmic.UI.xdnd{os.getpid()}"
        log_path = workdir / "gui.log"
        with log_path.open("wb") as logf:
            app_proc = subprocess.Popen(
                [
                    "flatpak", "run", "--user",
                    "--nosocket=wayland",
                    "--env=WAREHOUSE_COSMIC_UI_INSTANCE=" + instance,
                    "--env=WAREHOUSE_COSMIC_UI_ISOLATED=1",
                    "--command=env", "dev.nilltadios.WarehouseCosmic",
                    f"XDG_CONFIG_HOME={workdir / 'config'}",
                    "/app/bin/warehouse-cosmic",
                ],
                env=env, stdout=logf, stderr=subprocess.STDOUT,
            )

        # Wait for UI D-Bus bridge to come up.
        st = wait_state(env, instance, lambda s: not s.get("loading", True), "app finished loading", timeout=15.0)
        assert st["drag_over"] is False

        x11 = X11Client(display)
        target = None
        for _ in range(50):
            target = x11.find_xdnd_window()
            if target:
                break
            time.sleep(0.1)
        assert target is not None, f"Could not find XdndAware window on {display}"
        print(f"Found XdndAware Flatpak window: 0x{target:x}")

        # 1. Hover -> drop banner appears; Leave -> drop banner clears.
        payload = x11.begin_drag(target, [bundle])
        wait_state(env, instance, lambda s: s["drag_over"] is True, "drag_over=True on XdndPosition")
        print("PASS 1: X11 hover sets drag_over=True")

        x11.leave_drag(target)
        wait_state(env, instance, lambda s: s["drag_over"] is False, "drag_over=False on XdndLeave")
        print("PASS 2: X11 leave clears drag_over=False")

        # 2. Drop .flatpak with a space in the filename -> opens InstallFiles review, does not install.
        payload = x11.begin_drag(target, [bundle])
        wait_state(env, instance, lambda s: s["drag_over"] is True, "drag_over=True before drop")
        x11.drop_drag(target, payload)
        st = wait_state(
            env, instance,
            lambda s: s["drag_over"] is False and s.get("modal") == "install-files",
            "install-files modal after X11 drop",
        )
        assert st["install_files"] == [str(bundle.resolve())], f"Unexpected install_files: {st['install_files']}"
        assert st["busy"] is False, "Drop must not start installing without confirmation"
        print(f"PASS 3: X11 drop opened install review for {st['install_files']} without installing")

        ui_cli(env, instance, "activate", "dialog.cancel")
        wait_state(env, instance, lambda s: s.get("modal") is None, "dialog cancelled")

        # 3. Drop .flatpakrepo -> opens Add Remote dialog.
        payload = x11.begin_drag(target, [repo_file])
        x11.drop_drag(target, payload)
        st = wait_state(
            env, instance,
            lambda s: s.get("modal") == "add-remote",
            "remote dialog after .flatpakrepo X11 drop",
        )
        print("PASS 4: X11 drop of .flatpakrepo opened Add Remote dialog")
        ui_cli(env, instance, "activate", "dialog.cancel")
        wait_state(env, instance, lambda s: s.get("modal") is None, "remote dialog cancelled")

        # 4. Drop unsupported file -> shows Unsupported File Type dialog.
        payload = x11.begin_drag(target, [note])
        x11.drop_drag(target, payload)
        st = wait_state(
            env, instance,
            lambda s: (s.get("dialog") or {}).get("title") == "Unsupported File Type",
            "Unsupported File Type dialog after .txt X11 drop",
        )
        print("PASS 5: X11 drop of unsupported file showed Unsupported File Type dialog")
        ui_cli(env, instance, "activate", "dialog.cancel")

        # 5. Drop mixed types -> shows Mixed File Types dialog.
        payload = x11.begin_drag(target, [bundle, repo_file])
        x11.drop_drag(target, payload)
        st = wait_state(
            env, instance,
            lambda s: (s.get("dialog") or {}).get("title") == "Mixed File Types",
            "Mixed File Types dialog after mixed X11 drop",
        )
        print("PASS 6: X11 drop of mixed files showed Mixed File Types dialog")

        ui_cli(env, instance, "close", "--apply")
        app_proc.wait(timeout=10)
        print("ALL X11 XDND FLATPAK CHECKS PASSED")
        return 0
    finally:
        if x11 is not None:
            try:
                x11.close()
            except Exception:
                pass
        if app_proc is not None and app_proc.poll() is None:
            app_proc.terminate()
            try:
                app_proc.wait(timeout=5)
            except Exception:
                app_proc.kill()
        if xvfb is not None:
            xvfb.terminate()
            try:
                xvfb.wait(timeout=5)
            except Exception:
                xvfb.kill()
        shutil.rmtree(workdir, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
