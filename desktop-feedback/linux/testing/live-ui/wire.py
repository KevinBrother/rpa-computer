"""Strict bounded fixture-side v1 reader; does not change the product codec."""
import json

LIMIT = 16384
BUDGET = 65536

class Failure(Exception):
    pass

class Inconclusive(Failure):
    pass

def require(condition, message):
    if not condition:
        raise Failure(message)

def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result

def validate(msg):
    require(isinstance(msg, dict), 'wire object required')
    kind = msg.get('type')
    keys = {'ready': {'type','version','capture_exclusion','pointer_feedback'},
            'heartbeat': {'type','version'}, 'stop': {'type','version','session'},
            'error': {'type','version','code'}}
    require(kind in keys and set(msg) == keys[kind], 'wire type/fields invalid')
    require(type(msg['version']) is int and msg['version'] == 1, 'wire version invalid')
    if kind == 'ready':
        require(msg['capture_exclusion'] == 'unsupported', 'exclusion must stay unsupported')
        require(msg['pointer_feedback'] is True, 'pointer capability missing')
    if kind == 'stop':
        session = msg['session']
        require(isinstance(session, dict) and set(session) == {'id','generation'}, 'invalid Stop session')
        require(isinstance(session['id'], str) and 0 < len(session['id']) <= 128
                and all(c in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.:'
                        for c in session['id']), 'invalid Stop ID')
        require(type(session['generation']) is int and 0 <= session['generation'] < 2**64,
                'invalid Stop generation')
    if kind == 'error':
        require(isinstance(msg['code'], str) and 0 < len(msg['code']) <= 64
                and all(c in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.:'
                        for c in msg['code']), 'invalid error code')
    return msg

class Decoder:
    def __init__(self):
        self.pending = bytearray()
        self.total = 0
        self.count = 0

    def feed(self, data):
        self.total += len(data)
        require(self.total <= BUDGET, 'wire evidence budget exceeded')
        self.pending.extend(data)
        result = []
        while b'\n' in self.pending:
            end = self.pending.index(10) + 1
            require(end <= LIMIT, 'wire frame >16KiB')
            line = bytes(self.pending[:end-1])
            del self.pending[:end]
            try:
                message = json.loads(line.decode('utf-8'), object_pairs_hook=unique,
                                     parse_constant=lambda _: (_ for _ in ()).throw(Failure('nonfinite JSON')))
            except (ValueError, UnicodeError) as exc:
                raise Failure('invalid JSON/UTF8') from exc
            result.append(validate(message))
            self.count += 1
            require(self.count <= 256, 'message count exceeded')
        require(len(self.pending) < LIMIT, 'unterminated/oversized frame')
        return result

    def finish(self):
        require(not self.pending, 'truncated output frame')

def split_prefill(raw, count):
    # Fixture-only 0xff bytes cannot occur in valid UTF8 NDJSON. Preserve raw too.
    require(raw.count(b'\xff') == count, 'fixture filler byte accounting mismatch')
    return raw.replace(b'\xff', b'')

def snapshot(sequence, generation=1, phase='executing', cleanup='not_needed', pointer=None):
    obj = dict(type='snapshot', version=1, sequence=sequence,
               session=dict(id='fixture-a', generation=generation), phase=phase, cleanup=cleanup,
               surface=dict(id='desktop', version='fixture-1920x1080', x=0, y=0, width=1920, height=1080),
               pointer=pointer)
    data = json.dumps(obj, separators=(',', ':'), allow_nan=False).encode() + b'\n'
    require(len(data) <= LIMIT, 'fixture frame >16KiB')
    return data
