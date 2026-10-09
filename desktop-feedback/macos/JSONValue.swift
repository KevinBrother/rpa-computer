import Foundation

struct WireFailure: Error { let code: String; init(_ code: String) { self.code = code } }

// A small bounded JSON parser preserves uint64 lexical precision and rejects
// duplicate keys, invalid UTF-8, invalid escapes and excessive nesting.
indirect enum JSONValue {
    case object([String: JSONValue]), string(String), number(String), null
    static func parse(_ data: Data) throws -> JSONValue {
        guard data.count <= 16_383, let text = String(data: data, encoding: .utf8) else { throw WireFailure("invalid_utf8_or_size") }
        var parser = JSONParser(bytes: Array(text.utf8))
        let result = try parser.value(depth: 0)
        parser.space()
        guard parser.index == parser.bytes.count else { throw WireFailure("invalid_json") }
        return result
    }
    func object() throws -> [String: JSONValue] {
        guard case .object(let object) = self else { throw WireFailure("invalid_fields") }; return object
    }
    func string() throws -> String {
        guard case .string(let text) = self else { throw WireFailure("invalid_fields") }; return text
    }
    func uint() throws -> UInt64 {
        guard case .number(let raw) = self, !raw.isEmpty, raw.utf8.allSatisfy({ $0 >= 48 && $0 <= 57 }), let n = UInt64(raw) else { throw WireFailure("invalid_unsigned_integer") }
        return n
    }
    func finite() throws -> Double {
        guard case .number(let raw) = self, let n = Double(raw), n.isFinite else { throw WireFailure("invalid_coordinate") }; return n
    }
    var isNull: Bool { if case .null = self { return true }; return false }
}

private struct JSONParser {
    let bytes: [UInt8]
    var index = 0
    mutating func space() { while index < bytes.count && [9, 10, 13, 32].contains(bytes[index]) { index += 1 } }
    mutating func consume(_ byte: UInt8) -> Bool {
        space(); if index < bytes.count && bytes[index] == byte { index += 1; return true }; return false
    }
    mutating func value(depth: Int) throws -> JSONValue {
        space()
        guard depth <= 8, index < bytes.count else { throw WireFailure("invalid_json") }
        if bytes[index] == 123 {
            index += 1; var object: [String: JSONValue] = [:]
            if consume(125) { return .object(object) }
            repeat {
                space(); let key = try string()
                guard object[key] == nil, object.count < 32, consume(58) else { throw WireFailure("invalid_fields") }
                object[key] = try value(depth: depth + 1)
                if consume(125) { return .object(object) }
            } while consume(44)
            throw WireFailure("invalid_json")
        }
        if bytes[index] == 34 { return .string(try string()) }
        if index + 4 <= bytes.count && Array(bytes[index..<index + 4]) == Array("null".utf8) { index += 4; return .null }
        // Booleans and arrays have no place in the inbound v1 schema.
        let start = index
        if bytes[index] == 45 { index += 1 }
        guard index < bytes.count else { throw WireFailure("invalid_json") }
        if bytes[index] == 48 { index += 1 }
        else {
            guard bytes[index] >= 49 && bytes[index] <= 57 else { throw WireFailure("invalid_json") }
            digits()
        }
        if index < bytes.count && bytes[index] == 46 { index += 1; let p = index; digits(); guard index > p else { throw WireFailure("invalid_json") } }
        if index < bytes.count && [69, 101].contains(bytes[index]) {
            index += 1
            if index < bytes.count && [43, 45].contains(bytes[index]) { index += 1 }
            let p = index; digits(); guard index > p else { throw WireFailure("invalid_json") }
        }
        return .number(String(decoding: bytes[start..<index], as: UTF8.self))
    }
    mutating func digits() { while index < bytes.count && bytes[index] >= 48 && bytes[index] <= 57 { index += 1 } }
    mutating func string() throws -> String {
        guard index < bytes.count && bytes[index] == 34 else { throw WireFailure("invalid_json") }
        let start = index; index += 1
        while index < bytes.count {
            let byte = bytes[index]; index += 1
            if byte == 34 {
                let data = Data(bytes[start..<index])
                guard let text = try? JSONDecoder().decode(String.self, from: data) else { throw WireFailure("invalid_json_string") }; return text
            }
            guard byte >= 32 else { throw WireFailure("invalid_json_string") }
            if byte == 92 {
                guard index < bytes.count else { throw WireFailure("invalid_json_string") }
                index += 1 // Native decoder verifies the escape/surrogates after bounded scanning.
            }
        }
        throw WireFailure("invalid_json_string")
    }
}
