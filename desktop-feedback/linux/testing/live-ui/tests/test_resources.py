"""FD cleanup on failed construction: real local pipes, no child or GUI launch."""
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch
from ipc import Renderer, Observer

class ResourcesTests(unittest.TestCase):
    def test_renderer_failed_spawn_closes_all_pipe_ends(self):
        fds=[]
        original=os.pipe
        def pipe():
            pair=original(); fds.extend(pair); return pair
        with tempfile.TemporaryDirectory() as directory:
            evidence=Mock(directory=Path(directory))
            owner=Mock(); owner.spawn.side_effect=FileNotFoundError('deliberate')
            with patch('ipc.os.pipe',side_effect=pipe):
                with self.assertRaises(FileNotFoundError): Renderer('missing',{},owner,evidence)
            self.assertEqual(len(fds),4)
            for fd in fds:
                with self.assertRaises(OSError): os.fstat(fd)

    def test_second_pipe_failure_closes_first_pipe(self):
        fds=[]
        original=os.pipe
        def pipe():
            if fds: raise OSError('second pipe unavailable')
            pair=original(); fds.extend(pair); return pair
        with tempfile.TemporaryDirectory() as directory:
            with patch('ipc.os.pipe',side_effect=pipe):
                with self.assertRaises(OSError): Renderer('missing',{},Mock(),Mock(directory=Path(directory)))
            for fd in fds:
                with self.assertRaises(OSError): os.fstat(fd)

    def test_observer_failed_spawn_closes_stderr(self):
        with tempfile.TemporaryDirectory() as directory:
            stream=(Path(directory)/'capture').open('wb')
            owner=Mock(); owner.spawn.side_effect=FileNotFoundError('deliberate')
            with patch('pathlib.Path.open',return_value=stream):
                with self.assertRaises(FileNotFoundError): Observer('missing',{},owner,Mock(directory=Path(directory)))
            self.assertTrue(stream.closed)

    def test_input_eof_timestamp_is_not_reset_on_cleanup(self):
        r=Renderer.__new__(Renderer)
        read,r.write_fd=os.pipe()
        try:
            with patch('ipc.time.monotonic',return_value=10): r.close_input()
            with patch('ipc.time.monotonic',return_value=20): r.close_input()
            self.assertEqual(r.eof_at,10)
        finally:
            os.close(read)
            if r.write_fd is not None: os.close(r.write_fd)

    def test_exit_timing_starts_at_real_parent_eof(self):
        r=Renderer.__new__(Renderer)
        r.eof_at=10
        r.ended=True
        r.proc=Mock(returncode=0)
        r.proc.poll.return_value=0
        r.evidence=Mock()
        with patch('ipc.time.monotonic',return_value=10.5): self.assertEqual(r.exit(),(0,.5))
