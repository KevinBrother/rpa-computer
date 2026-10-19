"""Bounded file-only stderr capture for our owned Tk fixture, not the product host."""
import hashlib
import threading


class BoundedFixtureLog:
    def __init__(self, path, limit=64 * 1024):
        self.path = path
        self.limit = limit
        self.handle = path.open("xb", buffering=0)
        self.received = self.saved = 0
        self.digest = hashlib.sha256()
        self.complete = False
        self.error = None
        self.lock = threading.Lock()
        self.thread = None

    def start(self, stream):
        self.thread = threading.Thread(target=self.drain, args=(stream,), daemon=True,
                                       name="owned-fixture-stderr")
        self.thread.start()

    def drain(self, stream):
        try:
            while True:
                # Spawn uses unbuffered pipes: read available bytes, not a full
                # buffered 4KiB, so Tcl bgerrors are saved before process shutdown.
                block = stream.read(4096)
                if not block:
                    break
                with self.lock:
                    self.received += len(block)
                    self.digest.update(block)
                    chunk = block[:max(0, self.limit - self.saved)]
                    if chunk:
                        self.handle.write(chunk)
                        self.saved += len(chunk)
                # Keep draining after the file cap; never block the fixture on stderr.
        except (OSError, ValueError) as exc:
            with self.lock:
                self.error = type(exc).__name__
        finally:
            self.handle.close()
            with self.lock:
                self.complete = True

    def snapshot(self):
        with self.lock:
            return {"file": self.path.name, "bytes_received": self.received, "bytes_saved": self.saved,
                    "bytes_dropped": self.received - self.saved, "sha256_received": self.digest.hexdigest(),
                    "complete": self.complete, "error_type": self.error}

    def finish(self):
        if self.thread is not None:
            self.thread.join(timeout=0.5)
        else:
            self.handle.close()
        return self.snapshot()
