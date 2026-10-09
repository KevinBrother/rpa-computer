import Foundation
import Darwin

struct LineResult { let bytes: Data?; let failure: String? }
struct BoundedLines {
    private var buffer = Data()
    private var overflow = false
    mutating func feed(_ chunk: Data) -> [LineResult] {
        var result: [LineResult] = []
        for b in chunk {
            if b == 10 {
                if !overflow {
                    if buffer.last == 13 { buffer.removeLast() }
                    result.append(LineResult(bytes: buffer, failure: nil))
                }
                buffer.removeAll(keepingCapacity: true); overflow = false
            } else if !overflow {
                if buffer.count == 16_383 {
                    overflow = true; buffer.removeAll(keepingCapacity: true)
                    result.append(LineResult(bytes: nil, failure: "frame_too_large"))
                }
                else { buffer.append(b) }
            }
        }
        return result
    }
    mutating func finish() -> LineResult? {
        let failed = overflow ? "frame_too_large" : buffer.isEmpty ? nil : "truncated_frame"
        buffer.removeAll(); overflow = false
        return failed.map { LineResult(bytes: nil, failure: $0) }
    }
}
struct LatestMailbox {
    private var pending: Snapshot?
    private var high: UInt64?
    mutating func offer(_ s: Snapshot) {
        if let high, s.sequence <= high { return }
        high = s.sequence; pending = s
    }
    mutating func take() -> Snapshot? { let value = pending; pending = nil; return value }
}

final class InputPump: @unchecked Sendable {
    private let lock = NSLock()
    private var mailbox = LatestMailbox()
    private var failure: String?
    private var eof = false
    func start() {
        DispatchQueue.global(qos: .userInitiated).async { [self] in
            var lines = BoundedLines(), chunk = [UInt8](repeating: 0, count: 4096)
            while true {
                let count = Darwin.read(STDIN_FILENO, &chunk, chunk.count)
                if count < 0 { if errno == EINTR { continue }; fail("stdin_read_failed"); return }
                if count == 0 {
                    if let final = lines.finish() { fail(final.failure!); return }
                    lock.lock(); eof = true; lock.unlock(); return
                }
                for line in lines.feed(Data(chunk.prefix(count))) {
                    if let code = line.failure { fail(code); return }
                    do {
                        let s = try Snapshot.parse(line.bytes!)
                        lock.lock(); mailbox.offer(s); lock.unlock()
                    } catch let e as WireFailure { fail(e.code); return }
                    catch { fail("invalid_frame"); return }
                }
            }
        }
    }
    private func fail(_ code: String) { lock.lock(); failure = code; lock.unlock() }
    func poll() -> (Snapshot?, String?, Bool) {
        lock.lock(); defer { lock.unlock() }; return (mailbox.take(), failure, eof)
    }
}

// stdout is written only by this thread. UI never blocks on a Host pipe.
final class OutputPump: @unchecked Sendable {
    private let condition = NSCondition()
    private var queue: [Data] = []
    private var closing: Int32?
    init() {
        signal(SIGPIPE, SIG_IGN)
        DispatchQueue.global(qos: .userInitiated).async { [self] in run() }
    }
    func send(_ frame: Data) {
        guard frame.count < 16_384 else {
            FileHandle.standardError.write(Data("outbound_frame_too_large\n".utf8)); _exit(74)
        }
        condition.lock()
        guard closing == nil else { condition.unlock(); return }
        if queue.count >= 32 {
            condition.unlock()
            FileHandle.standardError.write(Data("stdout_queue_overflow\n".utf8)); _exit(74)
        }
        queue.append(frame + Data([10])); condition.signal(); condition.unlock()
    }
    func finish(exitCode: Int32) {
        condition.lock(); closing = exitCode; condition.signal(); condition.unlock()
        // Do not let a blocked output stream leave the renderer resident after EOF.
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.3) { _exit(exitCode) }
    }
    private func run() {
        while true {
            condition.lock()
            while queue.isEmpty && closing == nil { condition.wait() }
            if queue.isEmpty, let code = closing { condition.unlock(); _exit(code) }
            let frame = queue.removeFirst(); condition.unlock()
            var offset = 0
            let ok = frame.withUnsafeBytes { raw -> Bool in
                while offset < frame.count {
                    let count = Darwin.write(STDOUT_FILENO, raw.baseAddress!.advanced(by: offset), frame.count - offset)
                    if count < 0 { if errno == EINTR { continue }; return false }
                    if count == 0 { return false }; offset += count
                }
                return true
            }
            if !ok { FileHandle.standardError.write(Data("stdout_write_failed\n".utf8)); _exit(74) }
        }
    }
}
