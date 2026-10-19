"""Conservative evidence gates, not native/provider completion claims."""
import math
from rules import check_pixels
from wire import Failure, Inconclusive


def exclude_running_age(spawn_before, eof_at, exit_observed_at):
    values=(spawn_before,eof_at,exit_observed_at)
    if not all(type(v) in (int,float) and math.isfinite(v) for v in values):
        raise Inconclusive('running-age exclusion lacks finite process timestamps')
    if not spawn_before <= eof_at <= exit_observed_at:
        raise Inconclusive('running-age exclusion has unordered process timestamps')
    interval=exit_observed_at-spawn_before
    if not interval < 1.0:
        raise Inconclusive('running1s queue-age remains possible; pre-spawn→exit bound is '+str(interval))
    return interval


def ring_image(image):
    try:
        check_pixels(image)
        xid=image.get('window')
        geometry=image.get('geometry')
        if (type(xid) is not int or xid<=0 or not isinstance(geometry,list)
                or len(geometry)!=4 or not all(type(v) is int for v in geometry)
                or geometry[2]<=0 or geometry[3]<=0):
            raise Inconclusive('ring image lacks exact XID/physical geometry')
    except (Failure,KeyError,TypeError) as exc:
        raise Inconclusive('visible ring pixel witness missing: '+str(exc)) from exc
    return xid


def ring_continuity(before, after, before_windows, after_windows, events):
    xid=ring_image(before)
    if ring_image(after)!=xid or before['geometry']!=after['geometry']:
        raise Inconclusive('ring XID/geometry changed across injection')
    for windows in (before_windows,after_windows):
        rings=[w for w in windows if w.get('role')=='ring']
        if len(rings)!=1 or rings[0].get('id')!=xid or rings[0].get('map_state')!=2:
            raise Inconclusive('same ring not viewable in both attribute witnesses')
    for event in events:
        if event.get('type') in ('unmap','map','destroy'):
            if type(event.get('window')) is not int:
                raise Inconclusive('mapping trace lacks a window identity')
            if event['window']==xid:
                raise Inconclusive('ring mapping transition during injection: '+event['type'])
    return xid
