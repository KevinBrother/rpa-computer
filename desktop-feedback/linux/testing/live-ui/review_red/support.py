"""Scripted adversarial observations. No subprocess/X/GUI; NOT native proof."""
import json
from types import SimpleNamespace
from unittest.mock import Mock
from case import Case
from wire import Decoder


class Clock:
    def __init__(self, now=100.0): self.now=now
    def read(self): return self.now


class BlockedRenderer:
    def __init__(self, clock, spawn_before, click_at, eof_at, exit_at):
        self.clock=clock
        self.spawn_before=spawn_before
        self.click_at=click_at
        self.planned_eof=eof_at
        self.planned_exit=exit_at
        self.exit_observed_at=None
        self.eof_at=None
        self.proc=SimpleNamespace(pid=1234,poll=lambda: None)
        self.raw=bytearray()
        self.before_prefill=0
        self.decoder=Decoder()
        self.ended=False
        self.drain_calls=[]
    def fill_output(self):
        self.clock.now=self.click_at
        return 4096
    def close_input(self):
        self.clock.now=self.planned_eof
        self.eof_at=self.clock.now
    def exit(self, drain=True):
        self.drain_calls.append(drain)
        self.clock.now=self.planned_exit
        self.exit_observed_at=self.clock.now
        return 65,self.exit_observed_at-self.eof_at
    def pump(self):
        self.raw.extend(b'\xff'*4096)
        self.ended=True


class BlockedCase:
    """Existing queue_stop()/backpressure() run in full against supplied observations."""
    def __init__(self, clock, spawn_before=100.0, click_at=100.10,
                 eof_at=100.20, exit_at=100.50):
        self.r=BlockedRenderer(clock,spawn_before,click_at,eof_at,exit_at)
        self.evidence=Mock()
        self.disabled='disabled-reference'
        self.o=SimpleNamespace(click=lambda *args: None, move=lambda *args: None,
            capture=lambda *args: dict(fingerprint=self.disabled))
        self.destroy_calls=0
    def start(self, **kwargs): return self
    def activate(self): pass
    def stop_point(self): return 1800,40
    def target_presses(self): return []
    def until(self, fn, *args, **kwargs):
        assert fn(), 'scripted disabled-state fixture must be complete'
    def image(self, *args): return dict(fingerprint=self.disabled)
    def destroyed(self): self.destroy_calls+=1


class RingSectionComplete(Exception):
    """Stops at the first unrelated heartbeat check; not accepted as a rejection."""


class RingObserver:
    target=7
    def __init__(self, clock, schedule):
        self.clock=clock
        self.schedule=schedule
        self.ring=43
        self.visible=True
        self.events=[dict(type='map',window=self.ring)]
        self.injected=0
    def windows(self):
        return [dict(id=41,role='status',x=10,y=10,width=352,height=48,map_state=2,input=[]),
                dict(id=42,role='stop',x=380,y=10,width=88,height=48,map_state=2,input=[[0,0,88,48]]),
                dict(id=self.ring,role='ring',x=578,y=378,width=44,height=44,
                     map_state=2 if self.visible else 0,input=[])]
    def inspect(self,pid): return dict(windows=self.windows(),focus=self.target,events=[])
    def capture(self,pid,role,tag=''):
        w=next(w for w in self.windows() if w['role']==role)
        return dict(present=True,viewable=w['map_state']==2,window=w['id'],
                    fingerprint='painted-'+role,colors=8,pixels=100,pattern_pixels=20,
                    geometry=[w['x'],w['y'],w['width'],w['height']],focus=self.target,events=[])
    def call(self,op,**args):
        assert op=='events', 'unexpected test observer operation'
        return dict(events=[],focus=self.target)
    def click(self,x,y):
        ring_click=y==400 and x in (600,616)
        if ring_click:
            self.injected+=1
            if self.schedule in ('unmap','unmap-remap'):
                self.visible=False
                self.events.append(dict(type='unmap',window=self.ring))
            elif self.schedule=='destroy-replace':
                self.events.append(dict(type='destroy',window=self.ring))
                self.ring+=10
                self.events.append(dict(type='map',window=self.ring))
            elif self.schedule=='unrelated-unmap':
                self.events.append(dict(type='unmap',window=999))
        self.events.extend([dict(type='press',window=self.target,x=x,y=y),
                            dict(type='release',window=self.target)])
        if ring_click and self.schedule=='unmap-remap':
            self.visible=True
            self.events.append(dict(type='map',window=self.ring))
        self.clock.now+=.01
    def snapshot(self,data):
        if json.loads(data)['pointer']:
            if not self.visible: self.events.append(dict(type='map',window=self.ring))
            self.visible=True


def ring_case(clock,schedule):
    # Keep real Case.image/inspect/until/clickthrough and cases_ui.feedback wiring.
    c=Case.__new__(Case)
    c.sequence=0
    c.focus_start=0  # This harness starts in the running phase; retain all events.
    c.o=RingObserver(clock,schedule)
    c.r=SimpleNamespace(proc=SimpleNamespace(pid=1234,poll=lambda: None),
                        pump=lambda: None,send=c.o.snapshot,
                        wait=Mock(side_effect=RingSectionComplete))
    c.start=lambda: c
    c.activate=lambda: None
    return c
