"""Real private pipe parent, bounded protocol and explicit fixture-prefill accounting."""
import fcntl
import json
import os
import select
import subprocess
import time
from owned import Deadline
from wire import BUDGET, Decoder, Failure, Inconclusive, require, unique

class Renderer:
    def __init__(self, binary, env, owner, evidence, preclosed=False, prefill=False, label='AI Control'):
        self.evidence=evidence
        self.decoder=Decoder()
        self.messages=[]
        self.raw=bytearray()
        self.ended=False
        self.blocked=False
        self.prefill_count=0
        self.eof_at=None
        self.spawn_before=None
        self.exit_observed_at=None
        self.read_fd=self.write_fd=self.fill_fd=None
        self.stderr=None
        ir=outw=None
        try:
            ir, self.write_fd=os.pipe()
            self.read_fd, outw=os.pipe()
            self.stderr=(evidence.directory/'renderer.stderr').open('xb')
            if prefill:
                fcntl.fcntl(outw, fcntl.F_SETPIPE_SZ, 4096)
                self.fill_fd=os.dup(outw)
                os.set_blocking(self.fill_fd, False)
            if preclosed: self.close_input()
            # Lower bound precedes Popen/preexec/child startup, never Ready or click.
            self.spawn_before=time.monotonic()
            self.proc=owner.spawn([binary,'--label',label], env=env, stdin=ir, stdout=outw, stderr=self.stderr)
            os.set_blocking(self.read_fd, False)
            if self.write_fd is not None: os.set_blocking(self.write_fd, False)
            evidence.record('renderer_start', dict(pid=self.proc.pid, preclosed=preclosed, spawn_before=self.spawn_before))
        except BaseException:
            self.close()
            raise
        finally:
            if ir is not None: os.close(ir)
            if outw is not None: os.close(outw)
    def send(self, data):
        require(self.write_fd is not None, 'input already closed')
        deadline=Deadline(0.3)
        offset=0
        while offset<len(data):
            deadline.check()
            try: offset+=os.write(self.write_fd, data[offset:])
            except BlockingIOError: select.select([], [self.write_fd], [], min(0.02,deadline.remaining()))
        self.evidence.record('parent_bytes', data.decode('utf-8'))
    def pump(self):
        if self.ended: return
        while True:
            try: data=os.read(self.read_fd, 4096)
            except BlockingIOError: return
            if not data:
                self.ended=True
                if not self.blocked: self.decoder.finish()
                return
            self.raw.extend(data)
            require(len(self.raw)<=BUDGET+4096, 'stdout total budget')
            if not self.blocked:
                for msg in self.decoder.feed(data):
                    self.messages.append(msg)
                    self.evidence.record('renderer_message',msg)
    def wait(self, predicate, seconds=1):
        deadline=Deadline(seconds)
        while True:
            self.pump()
            if predicate(): return
            deadline.check()
            require(self.proc.poll() is None, 'renderer exited before expected message')
            select.select([self.read_fd], [], [], min(0.02, deadline.remaining()))
    def ready(self):
        self.wait(lambda: any(m['type']=='ready' for m in self.messages),3)
        require(sum(m['type']=='ready' for m in self.messages)==1, 'duplicate Ready')
        require(not any(m['type']=='error' for m in self.messages), 'renderer startup error')
    def stops(self): return [m['session'] for m in self.messages if m['type']=='stop']
    def close_input(self):
        if self.write_fd is not None:
            os.close(self.write_fd); self.write_fd=None
            self.eof_at=time.monotonic()
    def exit(self, drain=True):
        start=self.eof_at if self.eof_at is not None else time.monotonic()
        deadline=Deadline(2)
        deadline.end=start+2
        while self.proc.poll() is None:
            deadline.check()
            if drain: self.pump()
            time.sleep(0.005)
        # Upper bound immediately after poll observed termination, before I/O/logs.
        self.exit_observed_at=time.monotonic()
        if drain:
            while not self.ended:
                deadline.check(); self.pump()
                if not self.ended: time.sleep(0.005)
        elapsed=time.monotonic()-start
        require(elapsed<2, 'parent EOF-to-exit observation exceeded two seconds')
        self.evidence.record('renderer_exit', dict(rc=self.proc.returncode, elapsed=elapsed, exit_observed_at=self.exit_observed_at))
        return self.proc.returncode, elapsed
    def fill_output(self):
        require(self.fill_fd is not None, 'no fixture writer')
        self.pump()
        require(not self.decoder.pending, 'partial renderer frame before fixture prefill')
        self.blocked=True
        self.before_prefill=len(self.raw)
        deadline=Deadline(0.15)
        count=0
        for chunk in (b'\xff'*4096, b'\xff'):
            while True:
                deadline.check()
                try: count+=os.write(self.fill_fd,chunk)
                except BlockingIOError: break
                require(count<=4096, 'unexpected pipe capacity/prefill')
        os.close(self.fill_fd); self.fill_fd=None
        self.prefill_count=count
        require(count>0, 'no prefill bytes')
        self.evidence.record('fixture_prefill', dict(bytes=count, one_byte_eagain=True, capacity=fcntl.fcntl(self.read_fd,fcntl.F_GETPIPE_SZ)))
        return count
    def close_output(self):
        if self.fill_fd is not None: os.close(self.fill_fd); self.fill_fd=None
        os.close(self.read_fd); self.read_fd=None; self.ended=True
    def close(self):
        self.close_input()
        if self.fill_fd is not None: os.close(self.fill_fd); self.fill_fd=None
        if self.read_fd is not None: os.close(self.read_fd); self.read_fd=None
        if self.stderr is not None: self.stderr.close()
        (self.evidence.directory/'stdout.raw').write_bytes(self.raw)

class Observer:
    def __init__(self, binary, env, owner, evidence):
        self.evidence=evidence; self.buffer=bytearray(); self.id=0; self.events=[]
        self.proc=None
        self.stderr=(evidence.directory/'observer.stderr').open('xb')
        try:
            self.proc=owner.spawn([binary,'--artifacts',str(evidence.directory)],env=env,
                                  stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.stderr)
            os.set_blocking(self.proc.stdout.fileno(),False)
            os.set_blocking(self.proc.stdin.fileno(),False)
            self.info=self.response(0,3)
            if self.info.get('mapping_trace')!='map-unmap-destroy-v1':
                raise Inconclusive('observer mapping-trace capability absent; rebuild matching observer')
            self.target=self.info['target']
        except BaseException:
            try:
                if self.proc is not None: owner.stop(self.proc)
            finally: self.close()
            raise
    def response(self, expected, seconds=1):
        deadline=Deadline(seconds)
        while b'\n' not in self.buffer:
            deadline.check()
            readable,_,_=select.select([self.proc.stdout],[],[],min(0.05,deadline.remaining()))
            if not readable: continue
            chunk=os.read(self.proc.stdout.fileno(),4096)
            require(chunk, 'observer closed; dependency/native failure (see stderr)')
            self.buffer.extend(chunk)
            require(len(self.buffer)<=60001,'observer reply budget')
        line,_,tail=self.buffer.partition(b'\n'); self.buffer=bytearray(tail)
        value=json.loads(line.decode(),object_pairs_hook=unique)
        require(value['id']==expected and value['ok'] is True,'observer rejected operation: '+str(value.get('error','reply id')))
        result=value['result']
        self.evidence.record('observer',result)
        self.events.extend(result.get('events',[]))
        require(len(self.events)<=2048,'event trace budget')
        return result
    def call(self, op, **args):
        self.id+=1
        data=json.dumps(dict(id=self.id,op=op,**args),separators=(',',':')).encode()+b'\n'
        require(len(data)<=4096,'observer request budget')
        # <=PIPE_BUF on an empty command pipe; don't retry forever on wedged observer.
        require(os.write(self.proc.stdin.fileno(),data)==len(data),'observer partial command')
        return self.response(self.id)
    def inspect(self,pid): return self.call('inspect',pid=pid)
    def capture(self,pid,role,tag=''): return self.call('capture',pid=pid,role=role,tag=tag)
    def move(self,x,y): return self.call('move',x=int(x),y=int(y))
    def click(self,x,y):
        self.move(x,y); self.call('press'); self.call('release')
    def close(self):
        if self.proc is not None:
            self.proc.stdin.close(); self.proc.stdout.close()
        self.stderr.close()
