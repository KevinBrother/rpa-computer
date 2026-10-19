"""F1: negative assertions against actual current backpressure() acceptance path."""
import unittest
from unittest.mock import patch
import cases_shutdown
from wire import Inconclusive
from support import BlockedCase, Clock


class BackpressureAttributionTests(unittest.TestCase):
    def rejected(self, **timeline):
        clock=Clock()
        c=BlockedCase(clock,**timeline)
        with patch('cases_shutdown.time.monotonic',clock.read):
            with self.assertRaises(Inconclusive): cases_shutdown.backpressure(c)

    def test_running_age_rc65_cannot_be_r1_proof(self):
        # Every old scalar check passes: click→EOF .1s and EOF→exit1.1s.
        self.rejected(spawn_before=100,click_at=100.1,eof_at=100.2,exit_at=101.3)

    def test_older_heartbeat_short_eof_exit_still_not_r1_proof(self):
        # A queued heartbeat can be older than this click. Short .05s final latency
        # cannot distinguish its already-maturing running queue-age fault.
        self.rejected(spawn_before=100,click_at=102,eof_at=102.1,exit_at=102.15)

    def test_exact_one_second_spawn_exit_bound_is_not_exclusion(self):
        # The worker's >=1s branch is allowed at equality, so the bound is strict.
        self.rejected(spawn_before=100,click_at=100.6,eof_at=100.7,exit_at=101)

    def test_missing_pre_spawn_timestamp_is_inconclusive(self):
        self.rejected(spawn_before=None)

    def test_nonfinite_pre_spawn_timestamp_is_inconclusive(self):
        self.rejected(spawn_before=float('nan'))

    def test_fast_complete_window_can_remain_eligible(self):
        clock=Clock()
        c=BlockedCase(clock,spawn_before=100,click_at=100.1,eof_at=100.2,exit_at=100.5)
        with patch('cases_shutdown.time.monotonic',clock.read): cases_shutdown.backpressure(c)
        self.assertEqual(c.destroy_calls,1)
        self.assertEqual(c.r.drain_calls,[False])
        self.assertEqual(c.r.raw.count(b'\xff'),4096)
