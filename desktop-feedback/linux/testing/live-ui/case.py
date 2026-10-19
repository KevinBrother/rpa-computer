"""One case owns one observer and renderer; no shared desktop state."""
import time
from ipc import Observer, Renderer
from owned import Deadline, Evidence, Owned
from proof import ring_image, ring_continuity
from rules import check_focus, check_pixels, check_windows
from wire import require, snapshot

class Case:
    def __init__(self, directory, env, renderer, observer):
        self.evidence=Evidence(directory)
        self.owner=Owned()
        self.env=env
        self.renderer_binary=renderer
        self.observer_binary=observer
        self.o=None
        self.r=None
        self.sequence=0
        self.saved_windows=[]
        self.focus_start=0

    def start(self, ready=True, **options):
        self.o=Observer(self.observer_binary,self.env,self.owner,self.evidence)
        setup=self.o.move(100,800)
        self._arm_focus(setup)
        self.r=Renderer(self.renderer_binary,self.env,self.owner,self.evidence,**options)
        if ready:
            self.r.ready()
            windows=self.inspect()['windows']
            check_windows(windows)
            self.saved_windows=windows
            for w in windows:
                require(w['override_redirect'] and w['no_focus_hint'], 'nonactivating X attributes missing')
            self.image('status','ready')
            self.disabled=self.image('stop','ready')['fingerprint']
        return self

    def _arm_focus(self, setup):
        require(setup['focus']==self.o.target, 'setup target does not have keyboard focus')
        # The setup move already fenced/drained X; arm once before renderer spawn.
        # Keep the full history, but attribute only subsequent events to this run.
        self.focus_start=len(self.o.events)
        self.evidence.record('focus_guard_armed', dict(cursor=self.focus_start,target=self.o.target))

    def _check_focus(self, current):
        check_focus(current,self.o.target,self.o.events[self.focus_start:])

    def inspect(self):
        result=self.o.inspect(self.r.proc.pid)
        self._check_focus(result['focus'])
        return result

    def image(self, role, tag=''):
        result=self.o.capture(self.r.proc.pid,role,tag)
        check_pixels(result)
        self._check_focus(result['focus'])
        return result

    def until(self, fn, seconds=1, pump=True):
        deadline=Deadline(seconds)
        while True:
            if pump: self.r.pump()
            value=fn()
            if value: return value
            deadline.check()
            require(self.r.proc.poll() is None, 'renderer terminated before UI evidence')
            time.sleep(min(.01,deadline.remaining()))

    def settle(self, seconds=.12):
        deadline=Deadline(seconds)
        while time.monotonic()<deadline.end:
            self.r.pump()
            self.inspect()
            require(self.r.proc.poll() is None, 'renderer died while observing')
            time.sleep(.01)

    def send(self, **options):
        self.sequence+=1
        self.r.send(snapshot(self.sequence,**options))

    def changed(self, role, before, tag, seconds=1, pump=True):
        def observed():
            image=self.o.capture(self.r.proc.pid,role)
            return image if image.get('fingerprint') not in (None,before) else None
        self.until(observed,seconds,pump)
        image=self.image(role,tag)
        require(image['fingerprint']!=before,'presentation reverted before evidence')
        return image

    def activate(self, **options):
        old=self.image('status')['fingerprint']
        self.send(**options)
        self.changed('status',old,'active')
        self.changed('stop',self.disabled,'enabled')

    def stop_point(self, edge=False):
        stop=next(w for w in self.inspect()['windows'] if w['role']=='stop')
        return (stop['x']+(1 if edge else stop['width']//2),
                stop['y']+(1 if edge else stop['height']//2))

    def target_presses(self):
        self.o.call('events')
        return [e for e in self.o.events if e['type']=='press' and e['window']==self.o.target]

    def clickthrough(self,x,y):
        before=len(self.target_presses())
        self.o.click(x,y)
        self.until(lambda: len(self.target_presses())==before+1,.3)
        event=self.target_presses()[-1]
        require((event['x'],event['y'])==(x,y),'wrong underlying click coordinates')
        self.inspect()

    def ring_clickthrough(self,x,y,tag):
        # Arm before the pre-image, not after injection or the final screenshot.
        cursor=len(self.o.events)
        before=self.o.capture(self.r.proc.pid,'ring',tag+'-before')
        ring_image(before)  # missing paint/viewability/XID means inconclusive
        self._check_focus(before['focus'])
        gx,gy,w,h=before['geometry']
        require((gx+w//2,gy+h//2)==(600,400),'physical ring position')
        before_windows=self.inspect()['windows']
        check_windows(before_windows)
        self.clickthrough(x,y)  # existing genuine target hit-testing assertions
        after=self.o.capture(self.r.proc.pid,'ring',tag+'-after')
        after_windows=self.inspect()['windows']
        # Native events command explicitly fences the X server BEFORE draining events.
        fence=self.o.call('events')
        self._check_focus(fence['focus'])
        ring_continuity(before,after,before_windows,after_windows,self.o.events[cursor:])

    def destroyed(self):
        require(self.inspect()['windows']==[], 'renderer windows remain after exit')
        if self.saved_windows:
            destroyed={e['window'] for e in self.o.events if e['type']=='destroy'}
            require({w['id'] for w in self.saved_windows}<=destroyed, 'missing X destroy evidence')

    def close(self):
        errors=[]
        if self.o and self.o.proc.poll() is None:
            try:
                self.o.call('release')
                self.o.call('quit')
                require(self.o.proc.wait(timeout=.5)==0,'observer graceful exit')
            except Exception as exc: errors.append('observer: '+str(exc))
        try: self.owner.close()
        except Exception as exc: errors.append('owned: '+str(exc))
        if self.r: self.r.close()
        if self.o: self.o.close()
        self.evidence.record('owned_cleanup',dict(errors=errors,children=[dict(pid=c.pid,rc=c.poll()) for c in self.owner.children]))
        self.evidence.close()
        require(not errors, '; '.join(errors))
