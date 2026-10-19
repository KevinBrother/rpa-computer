"""Phase-boundary RED: actual Case methods, no display/process/test-only focus model."""
from copy import deepcopy
import unittest
from focus_support import Harness, transient
from wire import Failure


class FocusBoundaryTests(unittest.TestCase):
    def accepted(self, operation):
        try: return operation()
        except Failure as exc:
            self.fail('valid phase-separated observations rejected: '+str(exc))

    def test_setup_history_does_not_fail_start_and_epoch_precedes_spawn(self):
        prefix = [dict(type='create',window=7),dict(type='map',window=7)] + transient()
        with Harness(prefix=prefix) as h:
            self.accepted(h.c.start)
            self.assertEqual(h.o.captures, [('status','ready'),('stop','ready')])
            self.assertEqual(h.at_spawn['cursor'], len(prefix))
            self.assertEqual(h.at_spawn['events'], prefix)
            self.assertEqual(h.o.events, prefix)  # setup evidence was not erased
            armed = [v for k,v in h.at_spawn['evidence'] if k == 'focus_guard_armed']
            self.assertEqual(len(armed), 1)
            self.assertEqual((armed[0]['cursor'],armed[0]['target']), (len(prefix),7))

    def test_bad_setup_focus_prevents_renderer_construction(self):
        with Harness(setup_focus=8) as h:
            with self.assertRaises(Failure): h.c.start(ready=False)
            self.assertEqual(h.spawns, 0)

    def test_transient_loss_inside_renderer_construction_is_rejected(self):
        with Harness(inject='spawn') as h:
            with self.assertRaisesRegex(Failure, 'transient focus loss'): h.c.start()
            self.assertEqual(h.spawns, 1)
            self.assertEqual(h.o.current, 7)

    def test_transient_loss_during_ready_wait_is_rejected(self):
        with Harness(inject='ready') as h:
            with self.assertRaisesRegex(Failure, 'transient focus loss'): h.c.start()
            self.assertEqual(h.ready_calls, 1)

    def test_runtime_rechecks_and_image_do_not_reset_focus_interval(self):
        with Harness() as h:
            self.accepted(h.c.start)
            cursor = h.c.focus_start
            h.o.events.extend(transient())
            saved = deepcopy(h.o.events)
            for action in [h.c.inspect, lambda: h.c.image('status'), h.c.inspect]:
                with self.assertRaisesRegex(Failure, 'transient focus loss'): action()
                self.assertEqual(h.c.focus_start, cursor)
                self.assertEqual(h.o.events, saved)

    def test_wrong_current_focus_rejects_even_without_focus_out(self):
        with Harness() as h:
            self.accepted(h.c.start)
            h.o.current = 8
            with self.assertRaisesRegex(Failure, 'stole keyboard focus'): h.c.inspect()

    def test_ring_pre_image_checks_runtime_transient_loss(self):
        with Harness(ring_fault='before') as h:
            self.accepted(h.c.start)
            with self.assertRaisesRegex(Failure, 'transient focus loss'):
                h.c.ring_clickthrough(600,400,'ring')
            self.assertTrue(h.o.injected)
            self.assertNotIn('click', h.o.calls)

    def test_ring_final_fence_checks_runtime_transient_loss(self):
        with Harness(ring_fault='final') as h:
            self.accepted(h.c.start)
            with self.assertRaisesRegex(Failure, 'transient focus loss'):
                h.c.ring_clickthrough(600,400,'ring')
            self.assertTrue(h.o.after_ring and h.o.injected)
            self.assertEqual(h.o.calls.count('click'), 1)

    def test_ring_guards_share_pre_spawn_epoch_not_entire_setup_history(self):
        with Harness(prefix=transient()) as h:
            h.c.start(ready=False)
            self.accepted(lambda: h.c.ring_clickthrough(600,400,'ring'))
            self.assertEqual(h.c.focus_start, 2)
            self.assertEqual(h.o.events[:2], transient())
            self.assertEqual(h.o.calls.count('click'), 1)

    def test_ready_false_establishes_same_pre_spawn_boundary(self):
        with Harness(prefix=transient()) as h:
            h.c.start(ready=False)
            self.assertEqual(h.ready_calls, 0)
            self.accepted(h.c.inspect)
            self.assertEqual(h.at_spawn['cursor'], 2)
            self.assertEqual(h.o.events, transient())

    def test_ready_false_still_rejects_spawn_time_transient_loss(self):
        with Harness(inject='spawn') as h:
            h.c.start(ready=False)
            with self.assertRaisesRegex(Failure, 'transient focus loss'): h.c.inspect()
            self.assertEqual(h.ready_calls, 0)

    def test_unrelated_and_mapping_history_remain_append_only(self):
        prefix = [dict(type='create',window=7),dict(type='map',window=7)]
        with Harness(prefix=prefix) as h:
            self.accepted(h.c.start)
            extra = transient(99) + [dict(type='press',window=7,x=600,y=400),
                                      dict(type='destroy',window=99)]
            h.o.events.extend(extra)
            self.accepted(h.c.inspect)
            self.assertEqual(h.o.events, prefix+extra)
            self.assertEqual(len(h.c.target_presses()), 1)

    def test_clean_ring_path_reaches_existing_continuity_check(self):
        with Harness() as h:
            self.accepted(h.c.start)
            self.accepted(lambda: h.c.ring_clickthrough(616,400,'ring'))
            self.assertEqual(h.o.calls.count('click'), 1)
            self.assertIn(('ring','ring-before'), h.o.captures)
            self.assertIn(('ring','ring-after'), h.o.captures)
            self.assertEqual(h.o.calls[-1], 'events')
