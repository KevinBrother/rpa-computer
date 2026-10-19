"""Live cases use real XTEST hit-testing; image comparisons are not OCR."""
import time
from rules import check_clean_exit, check_control
from wire import require


def finish(c):
    c.r.close_input()
    rc,_=c.r.exit()
    require(rc==0 and c.r.ended,'normal renderer exit/drain')
    require(not any(m['type']=='error' for m in c.r.messages),'unexpected renderer error')
    require(sum(m['type']=='ready' for m in c.r.messages)==1,'exactly one Ready')
    c.destroyed()


def feedback(c):
    c.start()
    c.activate()
    status=next(w for w in c.inspect()['windows'] if w['role']=='status')
    c.clickthrough(status['x']+status['width']//2,status['y']+24)
    for index, (x,y) in enumerate([(600,400),(616,400)]):
        # Refresh separately so observer latency cannot silently expire the ring.
        start=time.monotonic()
        c.send(pointer=dict(x=600,y=400,kind='click'))
        c.until(lambda: c.o.capture(c.r.proc.pid,'ring').get('viewable'),.45)
        c.ring_clickthrough(x,y,'ring-'+str(index))
        require(time.monotonic()-start<.65,'ring hit-testing outside visible lifetime')
    # Keep Ready independent from heartbeats and check active UI still responds.
    c.r.wait(lambda: any(m['type']=='heartbeat' for m in c.r.messages),1.5)
    previous=c.image('status')['fingerprint']
    for phase in ('idle','paused','stopping'):
        c.send(phase=phase)
        previous=c.changed('status',previous,phase)['fingerprint']
    c.send(phase='faulted',cleanup='failed')
    failed=c.changed('status',previous,'cleanup-failed')['fingerprint']
    # Closed disables Stop: that visible change is an independent applied-state fence.
    old_stop=c.image('stop')['fingerprint']
    c.send(phase='closed',cleanup='failed')
    c.changed('stop',old_stop,'closed-disabled')
    closed=c.image('status','failed-closed')
    require(closed['fingerprint']==failed,'failed cleanup presentation lost on Closed')
    c.send(phase='closed',cleanup='released')
    c.changed('status',failed,'released-closed')
    require(not c.r.stops(),'clickthrough generated Stop')
    finish(c)
    check_clean_exit(c.r.proc.returncode,c.r.ended,c.r.messages)


def stop_once(c):
    c.start(); c.activate()
    before=len(c.target_presses())
    c.o.click(*c.stop_point(edge=True))
    c.r.wait(lambda: len(c.r.stops())>=1)
    c.o.move(100,800)
    c.until(lambda: c.o.capture(c.r.proc.pid,'stop').get('fingerprint')==c.disabled)
    c.image('stop','requested-disabled')
    c.o.click(*c.stop_point())
    c.settle()
    check_control(c.r.stops(),1)
    require(len(c.target_presses())==before,'Stop click leaked into target')
    finish(c)
    check_control(c.r.stops(),1)


def stale_grant(c):
    c.start(); c.activate()
    before=len(c.target_presses())
    normal=c.image('stop')['fingerprint']
    c.o.move(*c.stop_point())
    hover=c.changed('stop',normal,'hover')['fingerprint']
    c.o.call('press')
    c.changed('stop',hover,'pressed')  # actual renderer processed g1 press
    old_status=c.image('status')['fingerprint']
    c.send(generation=2,phase='paused')
    c.changed('status',old_status,'new-grant')  # g2 presentation acknowledged
    c.o.call('release')
    c.settle()
    require(not c.r.stops(),'stale g1 press produced a control')
    c.o.click(*c.stop_point())
    c.r.wait(lambda: len(c.r.stops())>=1)
    c.o.click(*c.stop_point())
    c.settle()
    check_control(c.r.stops(),2)
    require(len(c.target_presses())==before,'Stop input leaked into target')
    finish(c)
    check_control(c.r.stops(),2)


def unicode(c):
    # Deliberately modest script/font profile. Not a CJK/Arabic coverage claim.
    c.start(label='Contrôle Δ')
    c.activate()
    c.image('status','unicode-latin-greek')
    finish(c)
