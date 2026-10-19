"""Fixture rule tests; no display/renderer process is started by this module."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from rules import check_focus, check_pixels, check_clean_exit, check_control
from owned import Deadline, sha256_file
from wire import Failure

class AcceptanceTests(unittest.TestCase):
    def test_pixels_must_not_be_only_background(self):
        facts = dict(present=True, viewable=True, colors=3, pixels=100, pattern_pixels=30)
        check_pixels(facts)
        for change in [dict(present=False), dict(viewable=False), dict(colors=2), dict(pattern_pixels=100)]:
            with self.assertRaises(Failure): check_pixels(dict(facts, **change))

    def test_transient_focus_theft_is_not_cured_by_returning_focus(self):
        check_focus(7, 7, [])
        with self.assertRaises(Failure): check_focus(7, 7, [{'type':'focus_out','window':7}])
        with self.assertRaises(Failure): check_focus(8, 7, [])

    def test_clean_exit_rejects_fault_stop_and_missing_eof(self):
        check_clean_exit(0, True, [])
        for rc, eof, messages in [(65,True,[]), (0,False,[]), (0,True,[{'type':'error'}]), (0,True,[{'type':'stop'}])]:
            with self.assertRaises(Failure): check_clean_exit(rc, eof, messages)

    def test_clean_exit_does_not_drop_committed_ready(self):
        check_clean_exit(0, True, [{'type':'ready'}])
        with self.assertRaises(Failure): check_clean_exit(0, True, [{'type':'ready'}]*2)

    def test_control_is_once_only_and_exact_generation(self):
        check_control([{'id':'fixture-a','generation':2}], 2)
        for stops in [[], [{'id':'fixture-a','generation':1}], [{'id':'fixture-a','generation':2}]*2]:
            with self.assertRaises(Failure): check_control(stops, 2)

    def test_hash_is_streamed_and_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'bytes'
            path.write_bytes(b'abc')
            self.assertEqual(sha256_file(path), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad')

    def test_deadline_never_returns_negative_timeout(self):
        with patch('owned.time.monotonic', return_value=10): deadline=Deadline(1)
        with patch('owned.time.monotonic', return_value=12):
            with self.assertRaises(Failure): deadline.remaining()

    def test_deadline_valid_remaining(self):
        with patch('owned.time.monotonic', return_value=10): deadline=Deadline(1)
        with patch('owned.time.monotonic', return_value=10.5): self.assertEqual(deadline.remaining(),0.5)
