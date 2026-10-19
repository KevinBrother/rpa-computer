import unittest
from wire import Decoder, Failure, snapshot, split_prefill

READY = b'{"type":"ready","version":1,"capture_exclusion":"unsupported","pointer_feedback":true}\n'

class WireTests(unittest.TestCase):
    def test_fragmented_ready(self):
        d = Decoder()
        self.assertEqual(d.feed(READY[:7]), [])
        self.assertEqual(d.feed(READY[7:])[0]['type'], 'ready')
        d.finish()

    def test_duplicate_keys_rejected(self):
        with self.assertRaises(Failure):
            Decoder().feed(b'{"type":"heartbeat","version":1,"version":1}\n')

    def test_unknown_fields_rejected(self):
        with self.assertRaises(Failure):
            Decoder().feed(b'{"type":"heartbeat","version":1,"extra":1}\n')

    def test_requested_is_never_accepted_as_exclusion(self):
        with self.assertRaises(Failure):
            Decoder().feed(READY.replace(b'unsupported', b'requested'))

    def test_bool_is_not_version(self):
        with self.assertRaises(Failure):
            Decoder().feed(b'{"type":"heartbeat","version":true}\n')

    def test_oversized_and_unterminated_lines_fail(self):
        with self.assertRaises(Failure):
            Decoder().feed(b'x' * 16384)
        d = Decoder()
        d.feed(b'{')
        with self.assertRaises(Failure): d.finish()

    def test_bad_utf8_and_bad_generation_fail(self):
        for data in [b'\xff\n', b'{"type":"stop","version":1,"session":{"id":"a","generation":true}}\n']:
            with self.assertRaises(Failure): Decoder().feed(data)

    def test_prefill_accounted_separately(self):
        self.assertEqual(split_prefill(b'abc\xff\xffdef', 2), b'abcdef')
        with self.assertRaises(Failure): split_prefill(b'abc\xff', 2)

    def test_snapshot_is_complete_and_bounded(self):
        frame = snapshot(3, generation=2, phase='paused')
        self.assertLessEqual(len(frame), 16384)
        self.assertIn(b'"pointer":null', frame)
        self.assertIn(b'"cleanup":"not_needed"', frame)

    def test_error_codes_are_bounded_ascii_tokens(self):
        for code in ['bad code','é','\\n']:
            import json
            data=json.dumps(dict(type='error',version=1,code=code)).encode()+b'\n'
            with self.assertRaises(Failure): Decoder().feed(data)
