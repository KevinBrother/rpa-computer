import unittest
from rules import check_windows, classify_backpressure, display_number
from wire import Failure

class RulesTests(unittest.TestCase):
    def test_display_names_are_not_remote_or_inherited(self):
        self.assertEqual(display_number(91), ':91')
        for value in [0, 1, -1, 65536, True, 'host:0']:
            with self.assertRaises(Failure): display_number(value)

    def test_only_full_88x48_stop_shape_accepted(self):
        windows = [dict(role='status', map_state=2, width=352, height=48, input=[]),
                   dict(role='stop', map_state=2, width=88, height=48, input=[[0, 0, 88, 48]]),
                   dict(role='ring', map_state=0, width=1, height=1, input=[])]
        check_windows(windows)
        windows[1]['input'] = [[1, 1, 86, 46]]
        with self.assertRaises(Failure): check_windows(windows)

    def test_duplicate_roles_rejected(self):
        with self.assertRaises(Failure): check_windows([{'role':'stop'}] * 3)

    def test_backpressure_requires_each_observation(self):
        facts = dict(rc=65, prefill=4096, accounted=4096, eagain=True,
                     disabled=True, click_to_eof=0.15, elapsed=0.25)
        classify_backpressure(**facts)
        for key, value in [('rc',0), ('prefill',0), ('accounted',1), ('eagain',False),
                           ('disabled',False), ('click_to_eof',1.2), ('elapsed',3)]:
            with self.assertRaises(Failure): classify_backpressure(**dict(facts, **{key:value}))
