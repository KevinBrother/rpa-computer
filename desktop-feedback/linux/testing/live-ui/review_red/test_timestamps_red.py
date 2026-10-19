"""F1 witness plumbing: real Renderer constructor/exit, mocked spawn only."""
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
from ipc import Renderer
from support import Clock


class TimestampWitnessTests(unittest.TestCase):
    def test_spawn_lower_bound_is_taken_before_spawn_not_after_ready(self):
        clock=Clock(100)
        def spawn(*args,**kwargs):
            clock.now=100.8
            return SimpleNamespace(pid=1234)
        with tempfile.TemporaryDirectory() as directory:
            evidence=Mock(directory=Path(directory))
            with patch('ipc.time.monotonic',clock.read):
                r=Renderer('not-launched',{},SimpleNamespace(spawn=spawn),evidence)
                try: self.assertEqual(getattr(r,'spawn_before',None),100)
                finally: r.close()

    def test_exit_upper_bound_is_captured_before_evidence_io(self):
        clock=Clock(100.5)
        r=Renderer.__new__(Renderer)
        r.eof_at=100.2
        r.proc=Mock(returncode=65)
        r.proc.poll.return_value=65
        r.evidence=Mock()
        def delayed_record(*args,**kwargs): clock.now=105
        r.evidence.record.side_effect=delayed_record
        with patch('ipc.time.monotonic',clock.read): r.exit(drain=False)
        self.assertEqual(getattr(r,'exit_observed_at',None),100.5)
