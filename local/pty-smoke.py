#!/usr/bin/env python3
"""Isolated real-binary Kitty U=1 smoke. Never prints captured terminal bytes.

Run: python3 local/pty-smoke.py [--compare /usr/bin/zellij]
No Cargo or real HOME/session is used. Linux /proc tags restrict cleanup to our
own processes. The --fixture mode is exclusively launched in our private pane.
"""
import argparse
import base64
import errno
import fcntl
import json
import os
from pathlib import Path
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import tty
import uuid
import zlib

ESC = b"\x1b"
ST = ESC + b"\\"
COLS, ROWS, CW, CH = 80, 24, 11, 24
IMAGE_ID, IMAGE_COLS, IMAGE_ROWS = 0x123456, 17, 2
PLACEHOLDER = "\U0010eeee".encode()


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def png():
    # A tiny synthetic RGBA PNG, generated without imaging dependencies.
    def chunk(kind, data):
        return struct.pack("!I", len(data)) + kind + data + struct.pack("!I", zlib.crc32(kind + data))
    ihdr = struct.pack("!2I5B", 2, 2, 8, 6, 0, 0, 0)
    pixels = b"\x00\xff\x00\x00\xff\x00\xff\x00\xff" + b"\x00\x00\x00\xff\xff\xff\xff\x00\xff"
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")


def fixture(root):
    # Never usable as an accidental terminal graphics demo.
    require(os.environ.get("ZELLIJ_PTY_SMOKE") == root.name, "fixture requires private smoke environment")
    require(os.isatty(0) and os.isatty(1), "fixture requires private pane PTY")
    tty.setraw(0)
    (root / "ready").touch()

    def wait_file(name):
        deadline = time.monotonic() + 25
        while not (root / name).exists():
            require(time.monotonic() < deadline, "fixture coordination timeout: " + name)
            time.sleep(0.02)

    # Input barrier: startup replies have reached the client before this byte.
    deadline = time.monotonic() + 15
    seen = b""
    while b"Z" not in seen:
        require(time.monotonic() < deadline, "fixture input barrier timeout")
        if select.select([0], [], [], 0.1)[0]:
            seen += os.read(0, 4096)
    os.write(1, ESC + b"_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA" + ST)
    reply = b""
    deadline = time.monotonic() + 3
    while ST not in reply and time.monotonic() < deadline:
        if select.select([0], [], [], 0.1)[0]:
            reply += os.read(0, 4096)
    (root / "probe.json").write_text(json.dumps({"reply_hex": reply.hex()}))
    wait_file("draw")
    header = f"a=T,f=100,q=2,U=1,p=1,i={IMAGE_ID},c={IMAGE_COLS},r={IMAGE_ROWS}".encode()
    out = ESC + b"[2J" + ESC + b"[H" + ESC + b"_G" + header + b";" + base64.b64encode(png()) + ST
    marks = [0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F,
             0x0346, 0x034A, 0x034B, 0x034C, 0x0350, 0x0351, 0x0352, 0x0357, 0x035B]
    for row in range(IMAGE_ROWS):
        out += f"\x1b[{row + 1};1H\x1b[38;2;18;52;86m".encode()
        for col in range(IMAGE_COLS):
            out += ("\U0010eeee" + chr(marks[row]) + chr(marks[col])).encode()
        out += ESC + b"[0m"
    os.write(1, out + ESC + b"[6;1HDRAWN")
    (root / "drawn").touch()
    wait_file("clear")
    os.write(1, ESC + b"[1;1H" + ESC + b"[2K" + ESC + b"[2;1H" + ESC + b"[2K" + ESC + b"[6;1HCLEARED")
    (root / "cleared").touch()
    wait_file("exit")


class Host:
    """Small streaming query responder + normal Kitty placement state model."""
    def __init__(self, fd):
        self.fd = fd
        self.carry = b""
        self.capture = bytearray()
        self.queries = []
        self.commands = []
        self.images = {}
        self.live = {}
        self.partial = {}
        self.cursor = (0, 0)
        self.positions = {}

    def reply(self, query, data):
        self.queries.append(query)
        os.write(self.fd, data)

    def apc(self, body):
        if not body.startswith(b"G"):
            return
        head, _, payload = body[1:].partition(b";")
        fields = dict(part.split("=", 1) for part in head.decode("ascii").split(",") if "=" in part)
        action = fields.get("a")
        if action == "q":
            image_id = fields.get("i", "")
            if image_id in ("31", "32"):
                self.reply("kitty-" + image_id, ESC + b"_Gi=" + image_id.encode() + b";OK" + ST)
            return
        self.commands.append(fields)
        if action in ("t", "T") or (action is None and self.partial):
            image_id = fields.get("i") or next(iter(self.partial))
            if action in ("t", "T"):
                self.partial[image_id] = [fields, bytearray()]
            first, data = self.partial[image_id]
            data.extend(payload)
            if fields.get("m", "0") == "0":
                raw = base64.b64decode(data)
                if first.get("o") == "z":
                    raw = zlib.decompress(raw)
                self.images[image_id] = (first, raw)
                del self.partial[image_id]
        elif action == "p":
            key = (fields.get("i"), fields.get("p"))
            self.live[key] = fields
            self.positions[key] = self.cursor
        elif action == "d":
            image_id, placement = fields.get("i"), fields.get("p")
            if fields.get("d", "a") in ("a", "A"):
                self.live.clear()
            elif fields.get("d") in ("i", "I"):
                self.live = {key: value for key, value in self.live.items()
                             if not (key[0] == image_id and (placement is None or key[1] == placement))}

    def csi(self, body):
        cup = re.fullmatch(rb"(\d+);(\d+)[Hf]", body)
        if cup:
            self.cursor = (int(cup[1]) - 1, int(cup[2]) - 1)
        elif body in (b"H", b"f"):
            self.cursor = (0, 0)
        fixed = {
            b"14t": ("pixels", f"\x1b[4;{ROWS * CH};{COLS * CW}t".encode()),
            b"16t": ("cells", f"\x1b[6;{CH};{CW}t".encode()),
            b"18t": ("text-size", f"\x1b[8;{ROWS};{COLS}t".encode()),
            b"c": ("DA1", ESC + b"[?62;22c"),
            b"0c": ("DA1", ESC + b"[?62;22c"),
            b">c": ("DA2", ESC + b"[>1;4000;0c"),
            b"6n": ("cursor", ESC + b"[1;1R"),
            b"?996n": ("theme", ESC + b"[?997;1n"),
        }
        if body in fixed:
            self.reply(*fixed[body])
        elif re.fullmatch(rb"\?\d+\$p", body):
            mode = body[1:-2]
            self.reply("mode-" + mode.decode(), ESC + b"[?" + mode + b";2$y")

    def osc(self, body):
        if body in (b"10;?", b"11;?"):
            color = b"ffff/ffff/ffff" if body.startswith(b"10") else b"0000/0000/0000"
            self.reply("color-" + body[:2].decode(), ESC + b"]" + body[:2] + b";rgb:" + color + ST)
        elif re.fullmatch(rb"4;\d+;\?", body):
            self.reply("palette", ESC + b"]" + body[:-1] + b"rgb:8080/8080/8080" + ST)
        elif body == b"7501;?":
            self.reply("program-status", ESC + b"]7501;?" + ST)

    def feed(self, data):
        self.capture.extend(data)
        require(len(self.capture) < 4_000_000, "unexpectedly large host output")
        data = self.carry + data
        self.carry = b""
        index = 0
        while index < len(data):
            start = data.find(ESC, index)
            if start < 0:
                break
            if start + 1 == len(data):
                self.carry = data[start:]
                break
            kind = data[start + 1:start + 2]
            if kind in (b"_", b"]", b"P"):
                end = data.find(ST, start + 2)
                terminator = 2
                if kind == b"]":
                    bell = data.find(b"\x07", start + 2)
                    if bell >= 0 and (end < 0 or bell < end):
                        end, terminator = bell, 1
                if end < 0:
                    self.carry = data[start:]
                    break
                body = data[start + 2:end]
                if kind == b"_":
                    self.apc(body)
                elif kind == b"]":
                    self.osc(body)
                index = end + terminator
            elif kind == b"[":
                match = re.search(rb"[\x40-\x7e]", data[start + 2:])
                if match is None:
                    self.carry = data[start:]
                    break
                end = start + 2 + match.end()
                self.csi(data[start + 2:end])
                index = end
            else:
                index = start + 2


def own_pids(tag):
    marker = ("ZELLIJ_PTY_SMOKE=" + tag).encode()
    found = []
    for path in Path("/proc").glob("[0-9]*/environ"):
        try:
            if marker in path.read_bytes().split(b"\x00"):
                found.append(int(path.parent.name))
        except (OSError, PermissionError):
            pass
    return found


def run(binary, expect_support):
    script = Path(__file__).resolve()
    # Short /tmp root keeps Unix socket paths below sockaddr_un limits.
    with tempfile.TemporaryDirectory(prefix="zj-pty-") as temporary:
        root = Path(temporary)
        for name in ("home", "config", "cache", "data", "runtime", "sock", "tmp"):
            (root / name).mkdir(mode=0o700)
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(root / "home"),
               "XDG_CONFIG_HOME": str(root / "config"), "XDG_CACHE_HOME": str(root / "cache"),
               "XDG_DATA_HOME": str(root / "data"), "XDG_RUNTIME_DIR": str(root / "runtime"),
               "TMPDIR": str(root / "tmp"), "ZELLIJ_SOCKET_DIR": str(root / "sock"),
               "ZELLIJ_PTY_SMOKE": root.name, "TERM": "xterm-kitty", "COLORTERM": "truecolor",
               "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "SHELL": "/bin/sh"}
        (root / "config/zellij").mkdir()
        config = root / "config.kdl"
        config.write_text('pane_frames false\nshow_startup_tips false\nshow_release_notes false\nsession_serialization false\nsupport_kitty_graphics_protocol true\n')
        (root / "data/zellij").mkdir()
        (root / "data/zellij/desktop_integration.json").write_text('{"flags":["SetupNeverShow"]}')
        layout = root / "layout.kdl"
        # JSON strings are also valid ordinary KDL strings.
        args = " ".join(json.dumps(value) for value in (str(script), "--fixture", str(root)))
        layout.write_text(f'layout {{\n    pane command={json.dumps(sys.executable)} {{\n        args {args}\n    }}\n}}\n')
        session = "pty-" + uuid.uuid4().hex[:12]
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, COLS * CW, ROWS * CH))

        def controlling_terminal():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        process = subprocess.Popen([str(binary), "--config", str(config), "--config-dir", str(root / "config/zellij"), "--new-session-with-layout", str(layout), "--session", session],
                                   stdin=slave, stdout=slave, stderr=slave, env=env, cwd=root,
                                   preexec_fn=controlling_terminal, close_fds=True)
        os.close(slave)
        host = Host(master)

        def pump(seconds):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                if select.select([master], [], [], min(0.05, max(0, deadline - time.monotonic())))[0]:
                    try:
                        data = os.read(master, 65536)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            return
                        raise
                    if not data:
                        return
                    host.feed(data)

        def until(predicate, what, seconds=8):
            deadline = time.monotonic() + seconds
            while not predicate():
                require(time.monotonic() < deadline, "timeout: " + what)
                require(process.poll() is None, "client exited before " + what)
                pump(0.05)

        result = {}
        try:
            until(lambda: (root / "ready").exists() and "DA1" in host.queries and "kitty-31" in host.queries,
                  "pane ready + startup Kitty handshake")
            # A real socket under this exact override is a positive isolation check.
            sockets = [path for path in (root / "sock").rglob("*") if path.is_socket()]
            require(sockets, "ZELLIJ_SOCKET_DIR override produced no private socket")
            os.write(master, b"Z")
            until(lambda: (root / "probe.json").exists(), "pane query reply")
            pane_probe = json.loads((root / "probe.json").read_text())["reply_hex"]
            require((ESC + b"_Gi=31;OK" + ST).hex() in pane_probe, "pane did not observe Kitty support")
            (root / "draw").touch()
            until(lambda: (root / "drawn").exists(), "fixture image output")
            if expect_support:
                until(lambda: len(host.live) >= 2, "two ordinary Kitty row placements")
            pump(0.6)
            drawn = list(host.live.values())
            drawn_keys = set(host.live)
            drawn_positions = [host.positions[key] for key in host.live]
            image_headers = [entry[0] for entry in host.images.values()]
            raw_placeholder = PLACEHOLDER in host.capture
            raw_u1 = any(command.get("U") == "1" for command in host.commands)
            if expect_support:
                require(len(drawn) == 2, "expected exactly two live row slices")
                require({int(p["y"]) for p in drawn} == {0, CH}, "both image rows must be rendered")
                require(set(drawn_positions) == {(0, 0), (1, 0)}, "row slices were not placed at separate host rows")
                require(all(int(p["w"]) == IMAGE_COLS * CW and int(p["h"]) == CH for p in drawn), "unexpected crop geometry")
                require(all(p.get("C") == "1" and p.get("i") != str(IMAGE_ID) for p in drawn), "not translated normal placements")
                require(host.images, "no actual image upload")
                for header, pixels in host.images.values():
                    require(header.get("f") == "32" and header.get("t") == "d", "upload is not direct RGBA")
                    require(len(pixels) == int(header["s"]) * int(header["v"]) * 4 and any(pixels), "invalid or empty decoded upload")
                    for row in range(IMAGE_ROWS):
                        offset = row * CH * IMAGE_COLS * CW * 4
                        band = pixels[offset:offset + CH * IMAGE_COLS * CW * 4]
                        require(any(band[3::4]), "image row has no visible alpha pixels")
                require(not raw_placeholder and not raw_u1, "raw Unicode/U=1 escaped to host")
            before_clear = len(host.commands)
            (root / "clear").touch()
            until(lambda: (root / "cleared").exists(), "fixture erase output")
            if expect_support:
                until(lambda: not host.live, "all row placements deleted after erase")
            pump(0.6)
            deletes = [command for command in host.commands[before_clear:] if command.get("a") == "d"]
            if expect_support:
                deleted_keys = {(command.get("i"), command.get("p")) for command in deletes}
                require(not host.live and drawn_keys <= deleted_keys, "stale placement after erase")
                require(not any(command.get("a") == "p" for command in host.commands[before_clear:]), "image re-placed after erase")
            else:
                require(not drawn, "comparison binary unexpectedly rendered U=1; update baseline expectation")
                require(raw_placeholder, "old baseline did not expose the expected unsupported placeholder behavior")
            result = {"binary": str(binary), "version": subprocess.check_output([str(binary), "--version"], env=env, cwd=root, timeout=3).decode().strip(),
                      "expected_support": expect_support, "private_socket_paths": [str(path.relative_to(root)) for path in sockets],
                      "pane_probe_OK": True, "queries_answered": host.queries, "capture_bytes": len(host.capture),
                      "uploads": image_headers, "drawn_placements": drawn, "host_cells_zero_based": drawn_positions,
                      "raw_placeholder": raw_placeholder, "raw_U1": raw_u1,
                      "erase_deletes": deletes, "live_after_clear": len(host.live), "status": "PASS"}
        except Exception as error:
            # JSON escapes every control character; no raw captured bytes reach stdout.
            result = {"binary": str(binary), "status": "FAIL", "error": str(error),
                      "queries_answered": host.queries, "commands": host.commands,
                      "fixture_ready": (root / "ready").exists(),
                      "own_commands": [Path(f"/proc/{pid}/cmdline").read_bytes().decode("utf-8", "backslashreplace") for pid in own_pids(root.name)],
                      "private_logs": [path.read_text(errors="backslashreplace")[-2500:] for path in (root / "tmp").rglob("*.log")],
                      "captured_head_escaped": bytes(host.capture[:1200]).decode("utf-8", "backslashreplace"),
                      "captured_tail_escaped": bytes(host.capture[-2000:]).decode("utf-8", "backslashreplace")}
        finally:
            # This CLI inherits ONLY our socket/cache/config paths and targets ONLY our unique name.
            try:
                subprocess.run([str(binary), "kill-session", session], env=env, cwd=root,
                               stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=3)
            except (OSError, subprocess.TimeoutExpired):
                pass
            try:
                pump(0.2)
            except (OSError, AssertionError):
                pass
            for sig in (signal.SIGTERM, signal.SIGKILL):
                for pid in own_pids(root.name):
                    try:
                        # pidfd prevents PID reuse between marker verification and signal.
                        pidfd = os.pidfd_open(pid)
                        try:
                            marker = ("ZELLIJ_PTY_SMOKE=" + root.name).encode()
                            if marker in Path(f"/proc/{pid}/environ").read_bytes().split(b"\x00"):
                                signal.pidfd_send_signal(pidfd, sig)
                        finally:
                            os.close(pidfd)
                    except (ProcessLookupError, FileNotFoundError):
                        pass
                time.sleep(0.1)
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
            os.close(master)
            result["remaining_tagged_processes"] = own_pids(root.name)
            if result["remaining_tagged_processes"]:
                result["status"] = "FAIL"
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parent.parent / "target/release/zellij")
    parser.add_argument("--compare", type=Path, help="optional older binary expected NOT to support Unicode placeholders")
    parser.add_argument("--fixture", type=Path, help=argparse.SUPPRESS)
    options = parser.parse_args()
    if options.fixture:
        fixture(options.fixture)
        return 0
    def stop(signum, _frame):
        # Ensure run()'s narrowly scoped cleanup also runs on a terminating signal.
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGHUP, stop)
    results = [run(options.binary.resolve(), True)]
    if options.compare:
        results.append(run(options.compare.resolve(), False))
    print(json.dumps(results, ensure_ascii=True, indent=2))
    return 0 if all(result["status"] == "PASS" for result in results) else 1


if __name__ == "__main__":
    sys.exit(main())
