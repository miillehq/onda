import AppKit
import Foundation

/// Launch the copied bundle using Launch Services and terminate only the returned application.
@main
struct InstallerLaunchCheck {
    private static let startupSeconds: UInt64 = 6
    private static let terminationPolls = 50
    private static let pollNanoseconds: UInt64 = 100_000_000

    @MainActor
    static func main() async throws {
        guard CommandLine.arguments.count == 2 else { throw failure("Expected an installed app path.") }
        let bundle = URL(fileURLWithPath: CommandLine.arguments[1]).resolvingSymlinksInPath().standardizedFileURL
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.createsNewApplicationInstance = true
        let app = try await NSWorkspace.shared.openApplication(at: bundle, configuration: configuration)
        guard app.bundleURL?.resolvingSymlinksInPath().standardizedFileURL.path == bundle.path else {
            throw failure("Launch Services returned another installation.")
        }
        defer { if !app.isTerminated { app.forceTerminate() } }
        try await Task.sleep(nanoseconds: startupSeconds * 1_000_000_000)
        guard !app.isTerminated else { throw failure("The installed application exited during startup.") }
        print("Launched the installed bundle at \(bundle.path), pid \(app.processIdentifier).")
        app.terminate()
        for _ in 0..<terminationPolls {
            if app.isTerminated { return }
            try await Task.sleep(nanoseconds: pollNanoseconds)
        }
        throw failure("The installed application did not quit.")
    }

    private static func failure(_ message: String) -> NSError {
        NSError(domain: "OndaInstallerCheck", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}
