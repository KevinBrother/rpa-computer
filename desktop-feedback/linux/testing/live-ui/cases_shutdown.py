"""Real parent EOF/output-failure cases, not fake transport receipts."""
import time
from owned import Deadline
from proof import exclude_running_age
from rules import check_clean_exit, classify_backpressure
from wire import Inconclusive, require, split_prefill


def clean_eof(c):
    c.start(); c.activate()
    c.r.pump()
    c.r.close_input()
    rc,_=c.r.exit()
    check_clean_exit(rc,c.r.ended,c.r.messages)
    c.destroyed()


def preclosed_eof(c):
    c.start(ready=False,preclosed=True)
    rc,_=c.r.exit()
    check_clean_exit(rc,c.r.ended,c.r.messages)
    c.destroyed()
    # Record all transient create/map events; do not discard in-flight Ready.
    c.evidence.record('startup_cancellation',dict(boundary='input_closed_before_spawn',
        ready=sum(m['type']=='ready' for m in c.r.messages),events=c.o.events))


def observed_startup_eof(c):
    c.start(ready=False)
    deadline=Deadline(2)
    seen=[]
    while not seen:
        deadline.check()
        require(c.r.proc.poll() is None,'renderer failed before startup observation')
        observation=c.inspect()
        seen=observation['windows']
        # Do not require partial construction: source ordering is covered by pure tests.
        if not seen: time.sleep(.002)
    c.saved_windows=seen
    c.evidence.record('startup_before_eof',dict(windows=seen,ready_drained=False))
    c.r.close_input()
    rc,_=c.r.exit()
    check_clean_exit(rc,c.r.ended,c.r.messages)
    c.destroyed()
    c.evidence.record('startup_coverage',dict(internal_boundary_deterministic=False,
        ready=sum(m['type']=='ready' for m in c.r.messages)))


def truncated_eof(c):
    c.start()
    c.r.send(b'{"type":"snapshot"')
    c.r.close_input()
    rc,_=c.r.exit()
    require(rc==65 and c.r.ended,'truncated input must fail65 with observed output EOF')
    require(not c.r.stops(),'truncated input generated Stop')
    c.destroyed()


def queue_stop(c):
    c.start(prefill=True); c.activate()
    point=c.stop_point()
    before=len(c.target_presses())
    count=c.r.fill_output()  # no reads after this until EOF exit observation
    start=time.monotonic()
    c.o.click(*point)
    c.o.move(100,800)
    try:
        c.until(lambda: c.o.capture(c.r.proc.pid,'stop').get('fingerprint')==c.disabled,
                .45,pump=False)
    except Exception as exc:
        raise Inconclusive('cannot establish processed Stop before EOF: '+str(exc)) from exc
    c.image('stop','blocked-request-disabled')
    require(len(c.target_presses())==before,'blocked Stop leaked to target')
    require(c.r.proc.poll() is None,'renderer already exited before EOF')
    c.r.close_input()
    elapsed=c.r.eof_at-start
    if elapsed>=.8: raise Inconclusive('processed-Stop-to-EOF timing budget exceeded')
    c.evidence.record('processed_stop_before_eof',dict(disabled=True,click_to_eof=elapsed,
        interpretation='disabled pixels after real click; enqueue-before-paint is source-checked'))
    return count,elapsed


def backpressure(c):
    count,click_to_eof=queue_stop(c)
    rc,elapsed=c.r.exit(drain=False)  # critical: reader stays OPEN, never drains early
    require(rc==65,'backpressure exit was clean/watchdog/signal, not drain failure65')
    # Now and only now drain/account real bytes. Duplicate writer was already closed.
    deadline=Deadline(.3)
    while not c.r.ended:
        deadline.check(); c.r.pump()
    after=bytes(c.r.raw[c.r.before_prefill:])
    renderer_tail=split_prefill(after,count)
    messages=c.r.decoder.feed(renderer_tail)
    c.r.decoder.finish()
    c.evidence.record('accounted_renderer_tail',messages)
    require(all(m['type']=='heartbeat' for m in messages),
            'non-heartbeat frame escaped the full pipe / duplicate Ready')
    classify_backpressure(rc=rc,prefill=count,accounted=after.count(b'\xff'),eagain=True,
                          disabled=True,click_to_eof=click_to_eof,elapsed=elapsed)
    c.destroyed()
    interval=exclude_running_age(getattr(c.r,'spawn_before',None), c.r.eof_at,
                                 getattr(c.r,'exit_observed_at',None))
    c.evidence.record('running_age_excluded',dict(spawn_before=c.r.spawn_before,
        eof_at=c.r.eof_at,exit_observed_at=c.r.exit_observed_at,
        whole_process_bound=interval,strict_threshold=1.0))


def output_closed(c):
    count,click_to_eof=queue_stop(c)
    # Same real queued Stop, but remove the sole read end immediately after EOF.
    require(c.r.proc.poll() is None,'renderer exited before output closure')
    c.r.close_output()
    rc,elapsed=c.r.exit(drain=False)
    require(rc==65 and elapsed<2,'closed-output drain did not report bounded failure65')
    c.evidence.record('output_reader_closed',dict(rc=rc,prefill=count,click_to_eof=click_to_eof,
        filler_recovered=False,syscall_errno_traced=False,
        scope='real no-reader/pending-write condition; no control-delivery claim'))
    c.destroyed()
