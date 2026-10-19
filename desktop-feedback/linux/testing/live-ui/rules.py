"""Pure acceptance rules. Unit passes are NOT native UI proof."""
from wire import require

def display_number(number):
    require(type(number) is int and 90 <= number <= 199, 'owned display number must be 90..199')
    return ':' + str(number)

def check_windows(windows):
    require(len(windows) == 3 and {w['role'] for w in windows} == {'status','stop','ring'},
            'exactly three PID-matched role windows required')
    for w in windows:
        if w['role'] == 'stop':
            require((w['width'], w['height'], w['map_state']) == (88,48,2), 'Stop geometry/viewability')
            require(w['input'] == [[0,0,88,48]], 'full separate Stop input region required')
        else:
            require(w['input'] == [], 'status/ring must be input transparent')
            if w['role'] == 'status': require(w['map_state'] == 2, 'status not viewable')

def classify_backpressure(*, rc, prefill, accounted, eagain, disabled, click_to_eof, elapsed):
    require(rc == 65, 'drain must be classified failure65, not clean/watchdog/signal')
    require(prefill > 0 and accounted == prefill and eagain, 'real EAGAIN accounting missing')
    require(disabled and 0 <= click_to_eof < 0.8, 'queued Stop evidence too late/absent')
    require(0 <= elapsed < 2, 'bounded EOF exit not observed')

def check_pixels(facts):
    require(facts.get('present') and facts.get('viewable'), 'real window not viewable')
    require(facts['colors'] > 2 and facts['pattern_pixels'] < facts['pixels'],
            'no visible painted UI distinct from target background')

def check_focus(current, target, events):
    require(current == target, 'renderer stole keyboard focus')
    require(not any(e['type'] == 'focus_out' and e.get('window') == target for e in events),
            'transient focus loss observed')

def check_clean_exit(rc, eof, messages):
    require(rc == 0 and eof, 'clean exit needs rc0 AND real output EOF/drain')
    require(not any(m['type'] in ('stop','error') for m in messages), 'unexpected control/error during EOF')
    require(sum(m['type']=='ready' for m in messages) <= 1, 'duplicate Ready')

def check_control(stops, generation):
    require(stops == [dict(id='fixture-a', generation=generation)], 'Stop not once-only/current grant')
