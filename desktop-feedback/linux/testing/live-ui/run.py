#!/usr/bin/env python3
"""Opt-in private-X fixture. Importing does not launch processes or a display."""
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import time
from case import Case
import cases_ui
import cases_shutdown
from owned import Deadline, Owned, PrivateX, binary, sha256_file
from wire import Failure, Inconclusive, require

CASES = {
    'feedback':cases_ui.feedback,
    'stop-once':cases_ui.stop_once,
    'stale-grant':cases_ui.stale_grant,
    'unicode':cases_ui.unicode,
    'clean-eof':cases_shutdown.clean_eof,
    'preclosed-eof':cases_shutdown.preclosed_eof,
    'startup-eof':cases_shutdown.observed_startup_eof,
    'truncated-eof':cases_shutdown.truncated_eof,
    'backpressure':cases_shutdown.backpressure,
    'output-closed':cases_shutdown.output_closed,
}


def arguments():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--ack-owned-private-xvfb',action='store_true',required=True)
    p.add_argument('--renderer',required=True)
    p.add_argument('--renderer-sha256',required=True)
    p.add_argument('--observer',required=True)
    p.add_argument('--observer-sha256',required=True)
    p.add_argument('--out',type=Path,required=True,help='NEW private artifact directory')
    p.add_argument('--display',type=int,default=91)
    p.add_argument('--cases',nargs='+',choices=list(CASES),default=list(CASES))
    p.add_argument('--image-id',required=True,help='independent runner supplied immutable image digest/ID')
    p.add_argument('--source-sha256',required=True,help='independent runner supplied frozen product archive digest')
    return p.parse_args()


def inventory(directory,env,owner):
    commands={
        'pkg-config': ['pkg-config','--modversion','cairo','pango','pangocairo','glib-2.0','xcb'],
        'font-list': ['fc-list','--format','%{family}: %{style}\n'],
        'xvfb-packages': ['dpkg-query','-W','xvfb','xauth','libxcb1','libpango-1.0-0','libcairo2'],
    }
    for name,argv in commands.items():
        require(shutil.which(argv[0]),'dependency blocker: '+argv[0]+' missing (not installed)')
        with (directory/(name+'.txt')).open('xb') as out, (directory/(name+'.stderr')).open('xb') as err:
            child=owner.spawn(argv,env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err)
            require(child.wait(timeout=2)==0,'inventory failed: '+name)
        require((directory/(name+'.txt')).stat().st_size<=1024*1024,'inventory output budget')


def expired(signum,frame):
    raise Failure('work deadline/signal '+str(signum))


def main():
    args=arguments()
    require(sys.platform=='linux' and sys.version_info>=(3,10),'Linux Python>=3.10 required')
    require(len(set(args.cases))==len(args.cases),'duplicate cases')
    renderer=binary(args.renderer,args.renderer_sha256)
    observer=binary(args.observer,args.observer_sha256)
    # Never reuse a directory, server, authentication cookie or process.
    args.out=args.out.absolute()
    args.out.mkdir(mode=0o700,parents=False,exist_ok=False)
    owner=Owned(); server=None; active=None
    result=dict(status='FAIL',capture_exclusion='unsupported',host_admission=False,
                qualification_scope='selected_private_xvfb_cases_only',manual_visual_review_required=True,
                python=sys.version,platform=platform.platform(),cases=[],
                renderer_sha256=args.renderer_sha256,observer_sha256=args.observer_sha256,
                supplied_image_id=args.image_id,supplied_source_sha256=args.source_sha256,
                provenance_note='supplied image/source IDs; executable digests verified by fixture')
    whole=Deadline(120)
    start=time.monotonic()
    for sig in (signal.SIGALRM,signal.SIGINT,signal.SIGTERM): signal.signal(sig,expired)
    signal.setitimer(signal.ITIMER_REAL,whole.remaining())
    code=1
    try:
        server=PrivateX(args.out,args.display,owner)
        inventory(args.out,server.env,owner)
        for name in args.cases:
            server.assert_alive()
            signal.setitimer(signal.ITIMER_REAL,min(10,whole.remaining()))
            directory=args.out/name; directory.mkdir(mode=0o700)
            active=Case(directory,server.env,renderer,observer)
            begun=time.monotonic()
            entry=dict(case=name,status='FAIL')
            result['cases'].append(entry)
            try:
                CASES[name](active)
                server.assert_alive()
                entry['status']='PASS'
            except Inconclusive as exc:
                entry.update(status='INCONCLUSIVE',reason=str(exc)); raise
            except Exception as exc:
                entry['reason']=str(exc); raise
            finally:
                entry['work_seconds']=time.monotonic()-begun
                # Independent finite cleanup bounds; a case timeout never skips reaping.
                signal.setitimer(signal.ITIMER_REAL,0)
                current=active; active=None
                try: current.close()
                except Exception as exc:
                    entry.update(status='FAIL',cleanup_error=str(exc)); raise
                signal.setitimer(signal.ITIMER_REAL,whole.remaining())
        result['status']='PASS'
        code=0
    except Inconclusive as exc:
        result.update(status='INCONCLUSIVE',reason=str(exc)); code=2
    except Exception as exc:
        result.update(status='FAIL',reason=type(exc).__name__+': '+str(exc))
    finally:
        signal.setitimer(signal.ITIMER_REAL,0)
        # Repeated operator signals must not interrupt bounded owned cleanup.
        for sig in (signal.SIGINT,signal.SIGTERM): signal.signal(sig,signal.SIG_IGN)
        errors=[]
        if active:
            try: active.close()
            except Exception as exc: errors.append(str(exc))
        try: owner.close()
        except Exception as exc: errors.append(str(exc))
        if server:
            server.stderr.close()
            # Never unlink even our presumed socket/lock: let owned Xvfb remove them.
            if os.path.lexists(server.lock) or os.path.lexists(server.socket):
                errors.append('X socket/lock remains; not deleted by fixture')
        result['cleanup']=dict(errors=errors,children=[dict(pid=c.pid,rc=c.poll()) for c in owner.children])
        if errors: result['status']='FAIL'; code=1
        result['total_seconds']=time.monotonic()-start
        (args.out/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
        # Do not distribute the live auth cookie as an artifact.
        for name in ('Xauthority','Xauthority-c','Xauthority-l','Xauthority-n'):
            auth=args.out/name
            if auth.is_file() and not auth.is_symlink(): auth.unlink()
        files=sorted(p for p in args.out.rglob('*') if p.is_file() and not p.is_symlink())
        (args.out/'artifacts.sha256').write_text(''.join(sha256_file(p)+'  '+str(p.relative_to(args.out))+'\n' for p in files))
    print(json.dumps(dict(status=result['status'],artifacts=str(args.out),capture_exclusion='unsupported')))
    return code

if __name__=='__main__':
    try: sys.exit(main())
    except Exception as exc:
        print(type(exc).__name__+': '+str(exc),file=sys.stderr)
        sys.exit(1)
