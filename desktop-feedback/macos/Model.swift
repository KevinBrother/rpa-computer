import Foundation

struct FeedbackState {
    private(set) var snapshot: Snapshot?
    private(set) var stopRequested: Session?
    var canStop: Bool { snapshot?.session != nil && snapshot?.phase != "closed" && stopRequested != snapshot?.session }
    mutating func apply(_ next: Snapshot) -> Bool {
        if let old = snapshot, next.sequence <= old.sequence { return false }
        if next.session != snapshot?.session { stopRequested = nil }
        snapshot = next; return true
    }
    mutating func requestStop() -> Session? {
        guard canStop, let session = snapshot?.session else { return nil }
        stopRequested = session; return session
    }
    var status: String {
        guard let s = snapshot else { return "Waiting for Host" }
        // Cleanup uncertainty wins over any upbeat phase text.
        if s.cleanup == "failed" { return "Cleanup failed · inputs may remain held" }
        if s.cleanup == "unknown" { return "Cleanup unknown · not confirmed" }
        if s.cleanup == "pending" { return "Stopping · cleanup pending" }
        if s.phase == "closed" { return s.cleanup == "released" ? "Session closed · inputs released" : "Session closed · no cleanup required" }
        if stopRequested != nil { return "Stop requested · awaiting Host" }
        if s.phase == "faulted" { return s.session == nil ? "Host fault · no control session" : "Host fault · stop available" }
        guard s.session != nil else { return "Waiting for control session" }
        switch s.phase {
        case "starting": return "Connecting to Host"
        case "idle": return "Control held · waiting for Agent"
        case "observing": return "Observing · no input action"
        case "executing": return "Executing confirmed actions"
        case "paused": return "Control paused"
        case "stopping": return "Stopping · awaiting cleanup"
        default: return "State unknown"
        }
    }
}

enum Geometry {
    static func contains(_ s: Surface, x: Double, y: Double) -> Bool { x >= s.x && y >= s.y && x < s.x + s.width && y < s.y + s.height }
    static func appKitPointer(_ p: Pointer, surface s: Surface, frameLeft: Double, frameTop: Double) -> (x: Double, y: Double)? {
        guard contains(s, x: p.x, y: p.y) else { return nil }
        return (frameLeft + p.x - s.x, frameTop - (p.y - s.y))
    }
}

struct Style {
    let rgb: UInt32
    let label: String
    static func parse(_ args: [String]) throws -> Style {
        var rgb: UInt32 = 0x2563EB, label = "AI control", seen = Set<String>(), i = 0
        while i < args.count {
            let key = args[i]
            guard ["--accent", "--label"].contains(key), seen.insert(key).inserted, i + 1 < args.count else { throw WireFailure("invalid_options") }
            let value = args[i + 1]; i += 2
            if key == "--accent" {
                guard value.utf8.count == 7, value.first == "#", value.dropFirst().utf8.allSatisfy({ (48...57).contains($0) || (65...70).contains($0) || (97...102).contains($0) }), let parsed = UInt32(value.dropFirst(), radix: 16) else { throw WireFailure("invalid_accent") }
                rgb = parsed
            } else {
                // UTF-16 length bound matches C#; forbid control and formatting/bidi spoofing.
                guard !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, value.utf16.count <= 48, value.unicodeScalars.allSatisfy({ !CharacterSet.controlCharacters.contains($0) && $0.properties.generalCategory != .format && $0 != "\u{2028}" && $0 != "\u{2029}" }) else { throw WireFailure("invalid_label") }
                label = value
            }
        }
        return Style(rgb: rgb, label: label)
    }
}
