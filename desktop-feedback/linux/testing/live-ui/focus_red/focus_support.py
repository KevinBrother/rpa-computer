"""IO observations only; all focus decisions run through the real Case/rules."""
from contextlib import ExitStack
from copy import deepcopy
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import case


def transient(target=7):
    return [dict(type='focus_out', window=target), dict(type='focus_in', window=target)]


class MemoryEvidence:
    def __init__(self, directory):
        self.directory = directory
        self.records = []
    def record(self, kind, value): self.records.append((kind, deepcopy(value)))


class NoProcesses:
    def __init__(self): self.children = []
    def spawn(self, *args, **kwargs): raise AssertionError('unit must never spawn a process')


class ObserverInput:
    target = 7
    def __init__(self, prefix, setup_focus, ring_fault):
        self.prefix = deepcopy(prefix)
        self.setup_focus = setup_focus
        self.ring_fault = ring_fault
        self.events = []
        self.current = self.target
        self.calls = []
        self.captures = []
        self.after_ring = False
        self.injected = False
    def move(self, x, y):
        self.calls.append('setup_move')
        self.events.extend(deepcopy(self.prefix))
        return dict(focus=self.setup_focus, events=deepcopy(self.prefix))
    def windows(self):
        return [dict(id=41,role='status',x=16,y=16,width=352,height=48,map_state=2,input=[],
                     override_redirect=True,no_focus_hint=True),
                dict(id=42,role='stop',x=376,y=16,width=88,height=48,map_state=2,input=[[0,0,88,48]],
                     override_redirect=True,no_focus_hint=True),
                dict(id=43,role='ring',x=578,y=378,width=44,height=44,map_state=2,input=[],
                     override_redirect=True,no_focus_hint=True)]
    def inspect(self, pid):
        self.calls.append('inspect')
        return dict(focus=self.current, windows=self.windows())
    def capture(self, pid, role, tag=''):
        self.captures.append((role, tag))
        if role == 'ring' and tag.endswith('-before') and self.ring_fault == 'before':
            self.events.extend(transient())
            self.injected = True
        if role == 'ring' and tag.endswith('-after'): self.after_ring = True
        w = next(w for w in self.windows() if w['role'] == role)
        return dict(focus=self.current, present=True, viewable=True, window=w['id'],
                    geometry=[w['x'],w['y'],w['width'],w['height']], fingerprint='painted-'+role,
                    colors=8, pixels=100, pattern_pixels=20)
    def call(self, op, **kwargs):
        assert op == 'events', 'unexpected observer operation'
        self.calls.append('events')
        if self.after_ring and self.ring_fault == 'final' and not self.injected:
            self.events.extend(transient())
            self.injected = True
        return dict(focus=self.current, events=[])
    def click(self, x, y):
        self.calls.append('click')
        self.events.extend([dict(type='press',window=self.target,x=x,y=y),
                            dict(type='release',window=self.target)])


class Harness:
    def __init__(self, prefix=(), setup_focus=7, inject=None, ring_fault=None):
        self.o = ObserverInput(prefix, setup_focus, ring_fault)
        self.inject = inject
        self.spawns = 0
        self.ready_calls = 0
        self.at_spawn = None
        self.c = None
    def observer(self, *args): return self.o
    def renderer(self, *args, **kwargs):
        self.spawns += 1
        self.at_spawn = dict(cursor=self.c.focus_start, events=deepcopy(self.o.events),
                             evidence=deepcopy(self.c.evidence.records))
        if self.inject == 'spawn': self.o.events.extend(transient())
        def ready():
            self.ready_calls += 1
            if self.inject == 'ready': self.o.events.extend(transient())
        return SimpleNamespace(proc=SimpleNamespace(pid=1234,poll=lambda: None),
                               ready=ready,pump=lambda: None)
    def __enter__(self):
        self.stack = ExitStack()
        for name, replacement in [('Observer',self.observer),('Renderer',self.renderer),
                                  ('Evidence',MemoryEvidence),('Owned',NoProcesses)]:
            self.stack.enter_context(patch.object(case, name, replacement))
        self.c = case.Case(Path('/unused-focus-unit'), {}, 'unused-renderer', 'unused-observer')
        return self
    def __exit__(self, *exc): return self.stack.__exit__(*exc)
