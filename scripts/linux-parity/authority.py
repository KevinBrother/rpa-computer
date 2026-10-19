"""Second real host must refuse same-server ownership, even with distinct TMPDIR.

No contender input or initialization is sent; CLI lock acquisition
precedes protocol reads. A rejected contender must emit the structured lease_conflict code.
"""
import hashlib
import json
import os
import selectors
import subprocess
import time

from common import require, spawn, stop_owned


def authority(c):
    c.computer.open()
    before = c.state()
    baseline = c.snapshot("authority-before.json")
    tmp = c.ev.directory / "contender-tmp"
    tmp.mkdir(mode=0o700)
    environment = os.environ.copy()
    environment["TMPDIR"] = str(tmp)
    if c.args.authority_display:
        environment["DISPLAY"] = c.args.authority_display
    proc = spawn([c.args.host], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                 stderr=subprocess.PIPE, env=environment, bufsize=0)
    selector = selectors.DefaultSelector()
    data = {"stdout": bytearray(), "stderr": bytearray()}
    try:
        # Leave stdin open. Closing it could cause an unrelated successful exit
        # which would be false evidence of writer exclusion.
        for name in data:
            stream = getattr(proc, name)
            os.set_blocking(stream.fileno(), False)
            selector.register(stream, selectors.EVENT_READ, name)
        end = time.monotonic() + c.deadline.left(6)
        while proc.poll() is None or selector.get_map():
            remaining = min(c.deadline.left(), end - time.monotonic())
            require(remaining > 0, "contender did not reject ownership within six seconds")
            for key, _ in selector.select(remaining):
                block = os.read(key.fd, 65536)
                if block:
                    data[key.data].extend(block)
                    require(len(data[key.data]) <= 1024 * 1024, "contender output flood")
                else:
                    selector.unregister(key.fileobj)
        codes = []
        for line in data["stderr"].splitlines():
            try:
                item = json.loads(line)
                if isinstance(item, dict) and isinstance(item.get("error"), dict):
                    codes.append(item["error"].get("code"))
            except (ValueError, UnicodeError):
                pass
        c.ev.event("authority_contender", returncode=proc.returncode, error_codes=codes,
                   stdout_bytes=len(data["stdout"]), stderr_bytes=len(data["stderr"]),
                   stderr_sha256=hashlib.sha256(data["stderr"]).hexdigest(),
                   alternate_screen=bool(c.args.authority_display), separate_tmpdir=True)
        c.check(proc.returncode == 3 and "lease_conflict" in codes, "second_host_explicit_lease_conflict")
        c.check(not data["stdout"], "rejected_host_no_protocol_output")
        after = c.snapshot("authority-after.json")
        c.check(after == baseline, "contender_no_keymap_pointer_or_held_state_changes")
        c.wait(lambda s: s["seq"] == before["seq"] and s["entry"] == before["entry"] and
               s["text"] == before["text"] and s["clicks"] == before["clicks"],
               "contender_no_fixture_input", stable=0.3)
        # Confirm rightful owner remains usable; contender was never given input.
        c.click("entry")
        c.text("owner")
        c.wait(lambda s: s["entry"] == "owner", "owner_still_usable")
    finally:
        selector.close()
        stop_owned(proc)
