import AppKit
import Foundation
import Darwin

// This branch must remain before any NSApplication / screen / window access.
let arguments = Array(CommandLine.arguments.dropFirst())
if arguments == ["--self-test"] {
    do { try SelfTests.run(); exit(0) }
    catch let e as WireFailure { FileHandle.standardError.write(Data((e.code + "\n").utf8)); exit(1) }
    catch { FileHandle.standardError.write(Data("self_test_failed\n".utf8)); exit(1) }
}
let style: Style
do { style = try Style.parse(arguments) }
catch let e as WireFailure { FileHandle.standardOutput.write(WireOutput.error(e.code) + Data([10])); exit(64) }
catch { FileHandle.standardOutput.write(WireOutput.error("invalid_options") + Data([10])); exit(64) }

final class Renderer: NSObject, NSApplicationDelegate {
    let style: Style
    let output = OutputPump()
    let input = InputPump()
    var windows: FeedbackWindows?
    var state = FeedbackState()
    var timer: Timer?
    var lastHeartbeat = ProcessInfo.processInfo.systemUptime
    var pointerDeadline = 0.0
    var lastGeometryError: String?
    var lastGeometryCheck = 0.0
    var finished = false
    init(style: Style) { self.style = style }
    func applicationDidFinishLaunching(_ notification: Notification) {
        guard NSApp.setActivationPolicy(.accessory), let screen = NSScreen.screens.first else { fail("gui_initialization_failed"); return }
        windows = FeedbackWindows(style: style, screen: screen)
        windows?.onStop = { [weak self] in self?.requestStop() }
        guard windows?.initialized == true else { fail("gui_initialization_failed"); return }
        output.send(WireOutput.ready(exclusion: "unsupported"))
        FileHandle.standardError.write(Data("capture_exclusion_unsupported: Host capture integration required\n".utf8))
        lastHeartbeat = ProcessInfo.processInfo.systemUptime
        input.start()
        timer = Timer.scheduledTimer(withTimeInterval: 0.03, repeats: true) { [weak self] _ in self?.tick() }
    }
    func requestStop() {
        guard !finished, let session = state.requestStop() else { return }
        do { output.send(try WireOutput.stop(session)); _ = windows?.render(state) }
        catch { fail("stop_serialization_failed") }
    }
    func tick() {
        let (snapshot, error, eof) = input.poll()
        if let error { fail(error); return }
        if eof { finish(0); return } // EOF is not a statement about input cleanup.
        let now = ProcessInfo.processInfo.systemUptime
        let changed = snapshot.map { state.apply($0) } ?? false
        if changed { pointerDeadline = now + (snapshot?.pointer?.kind == "click" ? 0.65 : 0.35) }
        if changed || now - lastGeometryCheck >= 1 {
            if let code = windows?.render(state, showPointer: now < pointerDeadline) {
                if lastGeometryError != code { output.send(WireOutput.error(code)); lastGeometryError = code }
            } else { lastGeometryError = nil }
            lastGeometryCheck = now
        }
        if now >= pointerDeadline { windows?.expirePointer() }
        if now - lastHeartbeat >= 1 { output.send(WireOutput.heartbeat); lastHeartbeat = now }
    }
    // Shutdown attempts also route through stop, rather than independently exiting.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if finished { return .terminateNow }; requestStop(); return .terminateCancel
    }
    func fail(_ code: String) { output.send(WireOutput.error(code)); finish(65) }
    func finish(_ code: Int32) {
        guard !finished else { return }; finished = true
        timer?.invalidate(); windows?.shutdown(); output.finish(exitCode: code)
    }
}
let application = NSApplication.shared
let renderer = Renderer(style: style)
application.delegate = renderer
application.run()
