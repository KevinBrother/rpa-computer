"""CC-only unit tests for harness mechanics. No X server, Tk app, or real input.

Transport doubles here test the client, NEVER parity or product acceptance.
Authoring/py_compile does not execute this module.
"""
import json
from pathlib import Path
import struct
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
import zlib

from common import Deadline, Evidence, Failure
from protocol import Computer, image_header, validate
from rpc import Rpc


class SchemaTests(unittest.TestCase):
    def test_strict_types_and_unknown_fields(self):
        s = {"type": "object", "properties": {"n": {"type": "integer", "minimum": 1}},
             "required": ["n"], "additionalProperties": False}
        validate({"n": 1}, s)
        for value in ({"n": True}, {"n": 0}, {"n": 2, "extra": 1}, {}):
            with self.assertRaises(Failure):
                validate(value, s)

    def test_unknown_schema_fails_closed(self):
        with self.assertRaises(Failure):
            validate("x", {"type": "string", "pattern": "x"})

    def test_union_exactly_one(self):
        with self.assertRaises(Failure):
            validate("x", {"oneOf": [{"type": "string"}, {"const": "x"}]})

    def test_png_header_and_crc(self):
        data = b"IHDR" + struct.pack(">IIBBBBB", 640, 360, 8, 2, 0, 0, 0)
        blob = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + data + struct.pack(">I", zlib.crc32(data))
        self.assertEqual(image_header(blob), (640, 360))
        with self.assertRaises(Failure):
            image_header(blob[:-1] + bytes([blob[-1] ^ 1]))

    def test_mapping_uses_regions_not_desktop_ratio(self):
        computer = Computer(None, None)
        computer.obs = {"mapping_regions": [{"native_bounds": {"x": -800, "y": 50, "width": 800, "height": 600},
                                               "image_rect": {"x": 10, "y": 20, "width": 400, "height": 300}}]}
        self.assertEqual(computer.point(-400, 350), [210, 170])
        with self.assertRaises(Failure):
            computer.point(0, 350)


class RpcTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.clients = []

    def tearDown(self):
        for client in self.clients:
            client.dispose()
        self.temp.cleanup()

    def client(self, code, timeout=0.5, overall=3):
        server = self.root / ("server-" + str(len(self.clients)))
        server.write_text("#!" + sys.executable + "\nimport json, sys, time\n" + code)
        server.chmod(0o700)
        ev = Evidence(self.root / ("case-" + str(len(self.clients))))
        client = Rpc(str(server), Deadline(overall), ev, request_timeout=timeout)
        self.clients.append(client)
        return client

    def test_out_of_order_responses(self):
        client = self.client('a=json.loads(sys.stdin.readline()); b=json.loads(sys.stdin.readline())\n'
                             'for f in (b,a):\n'
                             ' print(json.dumps({"jsonrpc":"2.0","id":f["id"],"result":{"id":f["id"]}}),flush=True)\n')
        a, b = client.send("first"), client.send("second")
        self.assertEqual(client.receive(a), {"id": a})
        self.assertEqual(client.receive(b), {"id": b})

    def test_eof_does_not_wait_full_deadline(self):
        client = self.client('sys.stdin.readline()\n')
        started = time.monotonic()
        with self.assertRaises(Failure):
            client.call("eof")
        self.assertLess(time.monotonic() - started, 2)

    def test_request_deadline_and_no_retry(self):
        client = self.client('sys.stdin.readline(); time.sleep(10)\n', timeout=0.1)
        with self.assertRaises(TimeoutError):
            client.call("hang")
        with self.assertRaises(Failure):
            client.send("retry_must_not_write")

    def test_overall_caps_request_deadline(self):
        client = self.client('sys.stdin.readline(); time.sleep(10)\n', timeout=5, overall=0.2)
        started = time.monotonic()
        with self.assertRaises(TimeoutError):
            client.call("hang")
        self.assertLess(time.monotonic() - started, 1)

    def test_other_pending_request_keeps_own_deadline(self):
        client = self.client('sys.stdin.readline(); sys.stdin.readline(); time.sleep(10)\n', timeout=0.8)
        client.send("first")
        time.sleep(0.5)
        b = client.send("second")
        started = time.monotonic()
        with self.assertRaises(TimeoutError):
            client.receive(b)
        self.assertLess(time.monotonic() - started, 0.6)

    def test_nonreading_child_cannot_block_writer(self):
        client = self.client('time.sleep(10)\n', timeout=0.15)
        started = time.monotonic()
        with self.assertRaises(TimeoutError):
            client.call("big", {"owned_test_payload": "x" * (1024 * 1024)})
        self.assertLess(time.monotonic() - started, 1)

    def test_bad_json_unknown_id_and_partial_eof(self):
        for response in ('not-json\\n', '{"jsonrpc":"2.0","id":999,"result":{}}\\n', '{"jsonrpc":'):
            with self.subTest(response=response):
                client = self.client('sys.stdin.readline()\nsys.stdout.write("' + response.replace('"', '\\"') + '"); sys.stdout.flush()\n')
                with self.assertRaises((Failure, ValueError)):
                    client.call("bad")

    def test_frame_flood_bound(self):
        client = self.client('sys.stdin.readline(); sys.stdout.write("x"*4096); sys.stdout.flush(); time.sleep(10)\n')
        with patch("rpc.MAX_FRAME", 1024):
            with self.assertRaises(Failure):
                client.call("oversize")

    def test_stderr_not_raw_in_artifacts(self):
        client = self.client('f=json.loads(sys.stdin.readline())\n'
                             'sys.stderr.write("secret-env-sentinel\\n");sys.stderr.flush()\n'
                             'print(json.dumps({"jsonrpc":"2.0","id":f["id"],"result":{}}),flush=True)\n')
        client.call("one")
        client.eof_and_wait()
        client.dispose()
        self.clients.remove(client)
        self.assertNotIn("secret-env-sentinel", (self.root / "case-0" / "events.jsonl").read_text())


if __name__ == "__main__":
    unittest.main()
