import Foundation

struct Session: Equatable { let id: String; let generation: UInt64 }
struct Surface { let id: String; let version: String; let x: Double; let y: Double; let width: Double; let height: Double }
struct Pointer { let x: Double; let y: Double; let kind: String }
struct Snapshot {
    let sequence: UInt64
    let session: Session?
    let phase: String
    let cleanup: String
    let surface: Surface?
    let pointer: Pointer?
    static func parse(_ data: Data) throws -> Snapshot {
        let o = try fields(JSONValue.parse(data), ["type", "version", "sequence", "session", "phase", "cleanup", "surface", "pointer"])
        guard try o["type"]!.string() == "snapshot", try o["version"]!.uint() == 1 else { throw WireFailure("unsupported_protocol") }
        let phase = try o["phase"]!.string(), cleanup = try o["cleanup"]!.string()
        guard ["starting", "idle", "observing", "executing", "paused", "stopping", "faulted", "closed"].contains(phase), ["not_needed", "pending", "released", "failed", "unknown"].contains(cleanup) else { throw WireFailure("invalid_state") }
        var session: Session?, surface: Surface?, pointer: Pointer?
        if !o["session"]!.isNull {
            let s = try fields(o["session"]!, ["id", "generation"])
            session = Session(id: try nonempty(s["id"]!), generation: try s["generation"]!.uint())
        }
        if !o["surface"]!.isNull {
            let s = try fields(o["surface"]!, ["id", "version", "x", "y", "width", "height"])
            let x = try s["x"]!.finite(), y = try s["y"]!.finite(), w = try s["width"]!.finite(), h = try s["height"]!.finite()
            guard w > 0, h > 0, (x + w).isFinite, (y + h).isFinite else { throw WireFailure("invalid_surface") }
            surface = Surface(id: try nonempty(s["id"]!), version: try nonempty(s["version"]!), x: x, y: y, width: w, height: h)
        }
        if !o["pointer"]!.isNull {
            let p = try fields(o["pointer"]!, ["x", "y", "kind"])
            let kind = try p["kind"]!.string()
            guard ["move", "click", "drag"].contains(kind) else { throw WireFailure("invalid_pointer") }
            pointer = Pointer(x: try p["x"]!.finite(), y: try p["y"]!.finite(), kind: kind)
        }
        return Snapshot(sequence: try o["sequence"]!.uint(), session: session, phase: phase, cleanup: cleanup, surface: surface, pointer: pointer)
    }
    private static func fields(_ value: JSONValue, _ keys: [String]) throws -> [String: JSONValue] {
        let o = try value.object(); guard Set(o.keys) == Set(keys) else { throw WireFailure("invalid_fields") }; return o
    }
    private static func nonempty(_ value: JSONValue) throws -> String {
        let s = try value.string(); guard !s.isEmpty else { throw WireFailure("invalid_identifier") }; return s
    }
}

enum WireOutput {
    static func stop(_ session: Session) throws -> Data {
        // JSONSerialization retains NSNumber's unsigned 64-bit value, without Double conversion.
        try JSONSerialization.data(withJSONObject: ["type": "stop", "version": 1, "session": ["id": session.id, "generation": NSNumber(value: session.generation)]], options: [.sortedKeys, .withoutEscapingSlashes])
    }
    static func ready(exclusion: String) -> Data {
        Data("{\"type\":\"ready\",\"version\":1,\"capture_exclusion\":\"\(exclusion)\",\"pointer_feedback\":true}".utf8)
    }
    static let heartbeat = Data("{\"type\":\"heartbeat\",\"version\":1}".utf8)
    static func error(_ code: String) -> Data {
        // Callers only use bounded static codes; never echo inbound payloads.
        let safe = code.utf8.allSatisfy { ($0 >= 97 && $0 <= 122) || ($0 >= 48 && $0 <= 57) || $0 == 95 }
        return Data("{\"type\":\"error\",\"version\":1,\"code\":\"\(safe ? code : "renderer_error")\"}".utf8)
    }
}
