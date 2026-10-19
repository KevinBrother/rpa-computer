"""F2: run actual feedback ring section plus real Case clickthrough methods."""
import unittest
from unittest.mock import patch
import cases_ui
from wire import Inconclusive
from support import Clock, RingSectionComplete, ring_case


def exercise(schedule):
    clock=Clock()
    c=ring_case(clock,schedule)
    with patch('cases_ui.time.monotonic',clock.read):
        try: cases_ui.feedback(c)
        except RingSectionComplete: return c  # current path accepted both ring clicks
    raise AssertionError('feedback unexpectedly bypassed the ring-section boundary')


class RingContinuityTests(unittest.TestCase):
    def test_unmapped_ring_before_xtest_is_not_visible_clickthrough(self):
        with self.assertRaises(Inconclusive): exercise('unmap')

    def test_unmap_remap_gap_is_rejected_even_when_final_image_visible(self):
        with self.assertRaises(Inconclusive): exercise('unmap-remap')

    def test_destroyed_replaced_ring_is_not_same_xid_coverage(self):
        with self.assertRaises(Inconclusive): exercise('destroy-replace')

    def test_continuously_visible_ring_is_eligible(self):
        c=exercise('continuous')
        self.assertEqual(c.o.injected,2)

    def test_unrelated_window_unmap_does_not_invalidate_ring(self):
        c=exercise('unrelated-unmap')
        self.assertEqual(c.o.injected,2)
