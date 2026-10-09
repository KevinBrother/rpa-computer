import Foundation

enum GestureExporter {
    static func manifestJSON() -> String? {
        var root = CaseExporter.manifestJSONObject()
        var suites = root["suites"] as? [String: Any] ?? [:]
        for suite in ["multiclick", "drag", "scroll"] {
            let cases = GestureCatalog.cases(forSuite: suite) ?? []
            suites[suite] = ["total": cases.count, "cases": cases.map { c -> [String: Any] in
                ["id": c.id, "spec": c.spec, "instruction": c.instruction,
                 "platform_expected": ["macos": GesturePlatformNotes.note(suite: suite, caseId: c.id, platform: "macos"),
                                       "windows": GesturePlatformNotes.note(suite: suite, caseId: c.id, platform: "windows")]]
            }]
        }
        root["suites"] = suites
        guard let data = try? JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys]) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
