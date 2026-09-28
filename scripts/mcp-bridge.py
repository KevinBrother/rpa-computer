#!/usr/bin/env python3
"""Bridge a local MCP stdio client to the authenticated loopback TCP host.

Use case (Windows remote testing): the `computer-host --listen 127.0.0.1:PORT
--token-file PATH` process runs inside an INTERACTIVE Windows scheduled task.
SSH forwards that loopback port to this Mac; this bridge lets a local stdio
MCP client (e.g. `claude --mcp-config`) talk to it:

    ssh -L 127.0.0.1:8399:127.0.0.1:8399 acer-win   # coordinator-managed
    claude --mcp-config bridge-mcp.json ...          # command = this script

Protocol: newline-delimited JSON-RPC on stdio <-> TCP, with the token sent as
the FIRST line on the TCP connection (as the host requires — raw token line,
NOT a JSON frame; see .agents/reports/qa-fix-1.md).

Safety:
  * connects ONLY to 127.0.0.1 (refuses other hosts)
  * bounded frames (16 MiB/line) AND bounded queued output (64 MiB/direction):
    a peer that stops reading can never grow memory without limit; the bridge
    fails closed instead
  * token read from a file, never logged, never echoed to stdout, never in
    argv or error messages
  * half-close: when stdin EOFs and everything queued is flushed, the TCP
    write side is shut down cleanly (host sees EOF, runs its own cleanup);
    when TCP EOFs, remaining stdout queue is flushed before exiting
  * optional ABSOLUTE lifetime cap (--max-seconds) measured once from start

Stdlib only. Exit 0 on clean EOF, 1 on error.
"""

from __future__ import annotations

import argparse
import os
import selectors
import socket
import sys
import time

MAX_FRAME = 16 * 1024 * 1024
MAX_QUEUE = 64 * 1024 * 1024  # per-direction queued-but-unsent bytes
CONNECT_TIMEOUT = 10.0


def fail(msg: str) -> "None":
    # Never include the token or socket payloads in errors.
    print(f"mcp-bridge: {msg}", file=sys.stderr)
    sys.exit(1)


def read_token(path: str) -> bytes:
    try:
        with open(path, "rb") as f:
            token = f.read().strip()
    except OSError as e:
        fail(f"cannot read token file: {e.strerror or e}")
    if not token or b"\n" in token or len(token) > 4096:
        fail("token file malformed (empty, contains newline, or >4KiB)")
    return token


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", type=int, required=True, help="local forwarded loopback port")
    ap.add_argument("--token-file", required=True, help="path to file containing the auth token")
    ap.add_argument("--host", default="127.0.0.1", help="must be a loopback address")
    ap.add_argument("--max-seconds", type=float, default=0.0,
                    help="optional ABSOLUTE lifetime cap from startup (0 = unlimited)")
    args = ap.parse_args(argv)

    if args.host not in ("127.0.0.1", "::1", "localhost"):
        fail(f"refusing non-loopback host: {args.host}")
    if not (1 <= args.port <= 65535):
        fail("invalid port")

    token = read_token(args.token_file)

    try:
        sock = socket.create_connection((args.host, args.port), timeout=CONNECT_TIMEOUT)
    except OSError as e:
        fail(f"cannot connect to {args.host}:{args.port}: {e.strerror or e}")
    sock.setblocking(False)

    stdin_fd = sys.stdin.fileno()
    stdout_fd = sys.stdout.fileno()
    os.set_blocking(stdin_fd, False)
    os.set_blocking(stdout_fd, False)

    sel = selectors.DefaultSelector()
    sel.register(stdin_fd, selectors.EVENT_READ, "stdio")
    sel.register(sock, selectors.EVENT_READ, "tcp")

    tcp_out = bytearray(token + b"\n")      # auth first
    stdout_out = bytearray()
    stdin_eof = False
    stdin_wr_shutdown = False               # TCP write side already closed
    tcp_eof = False
    in_buf = bytearray()
    net_buf = bytearray()

    # Absolute deadline computed ONCE; activity must not extend it.
    deadline = time.monotonic() + args.max_seconds if args.max_seconds > 0 else None

    def flush_stdout() -> None:
        while stdout_out:
            try:
                n = os.write(stdout_fd, stdout_out)
            except BlockingIOError:
                return
            del stdout_out[:n]

    try:
        while True:
            if deadline is not None and time.monotonic() > deadline:
                fail("max-seconds exceeded (absolute deadline)")

            # Clean half-close: stdin EOF + fully flushed queue => SHUT_WR.
            if stdin_eof and not stdin_wr_shutdown and not tcp_out:
                try:
                    sock.shutdown(socket.SHUT_WR)
                except OSError:
                    pass
                stdin_wr_shutdown = True

            # TCP EOF: flush whatever remains to stdout, then exit cleanly.
            if tcp_eof:
                if not stdout_out:
                    return 0
                flush_stdout()
                if stdout_out:
                    # stdout back-pressured after peer EOF; bounded wait.
                    if len(stdout_out) > MAX_QUEUE:
                        fail("stdout queue overflow after TCP EOF")
                    time.sleep(0.05)
                    continue
                return 0

            events = sel.select(timeout=1.0)
            for key, _mask in events:
                if key.data == "stdio":
                    try:
                        chunk = os.read(stdin_fd, 65536)
                    except BlockingIOError:
                        continue
                    if not chunk:
                        stdin_eof = True
                        try:
                            sel.unregister(stdin_fd)
                        except Exception:  # noqa: BLE001
                            pass
                    else:
                        in_buf.extend(chunk)
                        while True:
                            nl = in_buf.find(b"\n")
                            if nl < 0:
                                break
                            frame = in_buf[: nl + 1]
                            del in_buf[: nl + 1]
                            if len(frame) > MAX_FRAME:
                                fail("oversized frame from stdio client")
                            if len(tcp_out) + len(frame) > MAX_QUEUE:
                                fail("TCP send queue overflow (host not reading)")
                            tcp_out.extend(frame)
                        if len(in_buf) > MAX_FRAME:
                            fail("unterminated oversized frame from stdio client")
                else:  # tcp
                    try:
                        chunk = sock.recv(65536)
                    except BlockingIOError:
                        continue
                    if not chunk:
                        tcp_eof = True
                        try:
                            sel.unregister(sock)
                        except Exception:  # noqa: BLE001
                            pass
                    else:
                        net_buf.extend(chunk)
                        while True:
                            nl = net_buf.find(b"\n")
                            if nl < 0:
                                break
                            frame = net_buf[: nl + 1]
                            del net_buf[: nl + 1]
                            if len(frame) > MAX_FRAME:
                                fail("oversized frame from host")
                            if len(stdout_out) + len(frame) > MAX_QUEUE:
                                fail("stdout send queue overflow (client not reading)")
                            stdout_out.extend(frame)
                        if len(net_buf) > MAX_FRAME:
                            fail("unterminated oversized frame from host")

            if tcp_out and not stdin_wr_shutdown:
                try:
                    sent = sock.send(tcp_out)
                    del tcp_out[:sent]
                except BlockingIOError:
                    pass
                except OSError:
                    fail("TCP write failed")
            if stdout_out:
                flush_stdout()
    finally:
        try:
            sel.close()
        finally:
            sock.close()


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except KeyboardInterrupt:
        sys.exit(130)
