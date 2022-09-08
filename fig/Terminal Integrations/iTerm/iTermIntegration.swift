//
//  iTermIntegration.swift
//  fig
//
//  Created by Matt Schrage on 6/9/21.
//  Copyright © 2021 Matt Schrage. All rights reserved.
//

import Cocoa

// swiftlint:disable:next type_name
class iTermIntegration: TerminalIntegrationProvider {
  static let `default` = iTermIntegration(bundleIdentifier: Integrations.iTerm)

  // MARK: - Installation
  fileprivate static let scriptName = "fig-iterm-integration"

  fileprivate static let iTermAutoLaunchDirectory =
    "\(NSHomeDirectory())/Library/Application Support/iTerm2/Scripts/AutoLaunch/"
  fileprivate static let autoLaunchScriptTarget = iTermAutoLaunchDirectory + scriptName + ".scpt"
  static let bundleAppleScriptFilePath = Bundle.main.path(forResource: scriptName, ofType: "scpt")!
  // Do we want to store the Applescript in the bundle or in withfig/fig? eg.
  // "\(NSHomeDirectory())/.fig/tools/\(scriptName).scpt"
  fileprivate static let plistVersionKey = "iTerm Version"
  fileprivate static let plistAPIEnabledKey = "EnableAPIServer"
  fileprivate static let minimumSupportedVersion = SemanticVersion(version: "3.4.0")!

  fileprivate static let legacyIntegrationPath = iTermAutoLaunchDirectory + scriptName + ".py"

  func uninstall() -> Bool {
    try? FileManager.default.removeItem(atPath: iTermIntegration.autoLaunchScriptTarget)
    return true
  }

  func install() -> InstallationStatus {
    guard NSWorkspace.shared.applicationIsInstalled(self.bundleIdentifier) else {
      return .applicationNotInstalled
    }
    // Check version number
    guard let iTermDefaults = UserDefaults(suiteName: self.bundleIdentifier),
          let version = iTermDefaults.string(forKey: iTermIntegration.plistVersionKey) else {

      return .failed(error: "Could not read iTerm plist file to determine version")
    }

    var cleanedVersion = version
    if cleanedVersion.contains("-nightly") {
      // remove nightly (3.4.20210701-nightly)
      cleanedVersion = cleanedVersion.stringByReplacingFirstOccurrenceOfString("-nightly", withString: "")
    } else if let range = cleanedVersion.range(of: "beta") {
      // remove beta (3.4.9beta1)
      cleanedVersion = String(cleanedVersion.prefix(upTo: range.lowerBound))
    }

    // Version Check
    guard let semver = SemanticVersion(version: cleanedVersion) else {
      return .failed(error: "iTerm version (\(version)) was invalid")
    }

    guard semver >= iTermIntegration.minimumSupportedVersion else {
      return .failed(error: "iTerm version \(version) is not supported. Must be " +
                      "\(iTermIntegration.minimumSupportedVersion.string) or above.")
    }

    // Update API preferences
    iTermDefaults.setValue(true, forKey: iTermIntegration.plistAPIEnabledKey)
    iTermDefaults.synchronize()

    // Create directory if it does not exist.
    try? FileManager.default.createDirectory(at: URL(fileURLWithPath: iTermIntegration.iTermAutoLaunchDirectory),
                                             withIntermediateDirectories: true,
                                             attributes: nil)
    // Delete existing file (in case, it was setup by when app was launched from DMG)
    try? FileManager.default.removeItem(atPath: iTermIntegration.autoLaunchScriptTarget)
    try? FileManager.default.createSymbolicLink(atPath: iTermIntegration.autoLaunchScriptTarget,
                                                withDestinationPath: iTermIntegration.bundleAppleScriptFilePath)

    let destination = try? FileManager.default.destinationOfSymbolicLink(
      atPath: iTermIntegration.autoLaunchScriptTarget
    )

    // Check if symlink exists and is pointing to the correct location
    guard destination == iTermIntegration.bundleAppleScriptFilePath else {
      return .failed(error: "Could not create symlink to '\(iTermIntegration.autoLaunchScriptTarget)'")
    }

    return .pending(event: .applicationRestart)
  }

  func verifyInstallation() -> InstallationStatus {

    guard self.applicationIsInstalled else {
      return .applicationNotInstalled
    }

    guard let symlinkDestination = try? FileManager.default.destinationOfSymbolicLink(
      atPath: iTermIntegration.autoLaunchScriptTarget
    ) else {
      return .failed(error: "AutoLaunch script does not exist at \(iTermIntegration.autoLaunchScriptTarget).")
    }

    guard symlinkDestination == iTermIntegration.bundleAppleScriptFilePath else {
      return .failed(error: "AutoLaunch script symlink points to the wrong file")
    }

    guard let iTermDefaults = UserDefaults(suiteName: self.bundleIdentifier) else {
      return .failed(error: "Could not read iTerm plist file to determine version")
    }

    let apiEnabled = iTermDefaults.bool(forKey: iTermIntegration.plistAPIEnabledKey)

    guard apiEnabled else {
      return .failed(error: "iTerm's Python API is not enabled.")
    }

    return .installed
  }

  // MARK: - Utilities

  override init(bundleIdentifier: String) {
    super.init(bundleIdentifier: bundleIdentifier)

    guard self.appIsInstalled else {
      print("iTerm is not installed.")
      return
    }

    socket.delegate = self
    ws.register(delegate: self)

    NSWorkspace.shared.notificationCenter.addObserver(self,
                                                      selector: #selector(didTerminateApplication),
                                                      name: NSWorkspace.didTerminateApplicationNotification,
                                                      object: nil)

    self.attemptToConnect()

  }

  var appIsInstalled: Bool {
    return NSWorkspace.shared.urlForApplication(withBundleIdentifier: self.bundleIdentifier) != nil
  }

  var appIsRunning: Bool {
    return NSWorkspace.shared.runningApplications.contains { $0.bundleIdentifier == self.bundleIdentifier }
  }

  fileprivate var sessionId: String? {
    didSet {
      guard let sessionId = sessionId else {
        return
      }

      Logger.log(message: "sessionId did changed to \(sessionId)", subsystem: .iterm)

      if let window = AXWindowServer.shared.allowlistedWindow, window.bundleId ?? "" == self.bundleIdentifier {
        ShellHookManager.shared.keyboardFocusDidChange(to: sessionId, in: window)

      }

    }
  }

  var currentSessionId: String? {
    guard appIsInstalled, self.socket.isConnected else {
      return nil
    }

    return self.sessionId
  }

  // MARK: - iTerm API
  static let apiCredentialsPath = "\(NSHomeDirectory())/.fig/tools/iterm-api-credentials"
  let socket = UnixSocketClient(
    path: "\(NSHomeDirectory())/Library/Application Support/iTerm2/private/socket",
    waitForNewline: false
  )

  // swiftlint:disable identifier_name
  let ws = WSFramer(isServer: false)

  // API
  var isConnectedToAPI = false {
    didSet {
      if isConnectedToAPI {
        // signal that iTerm Integration has been set up successfully
        self.runtimeValidationOccurred()
        // Remove legacy integration!
        try? FileManager.default.removeItem(at: URL(fileURLWithPath: iTermIntegration.legacyIntegrationPath))
      }
    }
  }

}

extension iTermIntegration {
  // https://gitlab.com/gnachman/iterm2/-/issues/9058#note_392824614
  // swiftlint:disable line_length
  // https://github.com/gnachman/iTerm2/blob/c52136b7c0bae545436be8d1441449f19e21faa1/sources/iTermWebSocketConnection.m#L50
  static let iTermLibraryVersion = 0.24
  fileprivate func handshakeRequest(cookie: String, key: String) -> String {
    let CRLF = "\r\n"

    let request = [
      "GET / HTTP/1.1",
      "connection: Upgrade",
      "upgrade: websocket",
      "origin: ws://localhost/",
      "host: localhost",
      "sec-websocket-protocol: api.iterm2.com",
      "sec-websocket-version: 13",
      "sec-websocket-key: \(key)",
      "x-iterm2-library-version: python \(iTermIntegration.iTermLibraryVersion)",
      "x-iterm2-cookie: \(cookie)",
      "x-iterm2-key: \(key)",
      "x-iterm2-disable-auth-ui: true"

    ]

    return request.joined(separator: CRLF) + CRLF + CRLF
  }

  @objc func didTerminateApplication(notification: Notification) {
    guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else {
      return
    }

    guard app.bundleIdentifier == self.bundleIdentifier else {
      return
    }

    Logger.log(message: "disconnecting socket because application was terminated.", subsystem: .iterm)
    self.disconnect()

  }

  func credentials() -> (String, String)? {
    guard FileManager.default.fileExists(atPath: iTermIntegration.apiCredentialsPath) else {
      Logger.log(
        message: "credentials file does not exist - this is likely because " +
          "Fig is newly installed and iTerm has not restarted yet!",
        subsystem: .iterm
      )
      return nil
    }

    guard let contents = try? String(contentsOfFile: iTermIntegration.apiCredentialsPath, encoding: .utf8) else {
      return nil
    }

    var allCredentials = contents.split(separator: "\n")

    guard allCredentials.count > 0 else {
      Logger.log(message: "no credentials available!", subsystem: .iterm)
      return nil
    }

    let currentCredentials = allCredentials.removeFirst()
    let tokens = currentCredentials.split(separator: " ").map {
      String($0).trimmingCharacters(in: .whitespacesAndNewlines)
    }
    guard tokens.count == 2 else {
      return nil
    }

    let updatedCredentialsList = allCredentials.joined(separator: "\n")
    do {
      try updatedCredentialsList.write(to: URL(fileURLWithPath: iTermIntegration.apiCredentialsPath),
                                       atomically: true,
                                       encoding: .utf8)
    } catch {
      Logger.log(
        message: "error writing updated credential list to \(iTermIntegration.apiCredentialsPath)",
        subsystem: .iterm
      )
    }

    return (tokens[0], tokens[1])

  }

  func disconnect() {
    self.socket.disconnect()
    self.ws.reset()
    self.isConnectedToAPI = false

  }

  func attemptToConnect() {
    Logger.log(message: "attempting to connect!", subsystem: .iterm)

    guard appIsRunning else {
      Logger.log(message: "target app is not running...", subsystem: .iterm)
      return
    }

    if socket.connect() {
      Logger.log(message: "connected to socket", subsystem: .iterm)

      guard let (cookie, key) =  credentials() else {
        Logger.log(message: "could not find credentials", subsystem: .iterm)

        self.disconnect()
        return
      }

      Logger.log(message: "Sending websocket handshake", subsystem: .iterm)

      Timer.delayWithSeconds(1) {
        self.socket.send(message: self.handshakeRequest(cookie: cookie, key: key))
        Logger.log(message: "Sent websocket handshake!", subsystem: .iterm)

      }

    } else {
      Logger.log(message: "Already connected...", subsystem: .iterm)
    }
  }
}

extension iTermIntegration: FramerEventClient {
  func frameProcessed(event: FrameEvent) {

    switch event {
    case .frame(let frame):
      guard let message = try? Iterm2_ServerOriginatedMessage(serializedData: frame.payload) else {
        Logger.log(message: "could not parse protobuf frame payload", subsystem: .iterm)
        return
      }

      guard message.error.count == 0 else {
        Logger.log(message: "API error - \(message.error)", subsystem: .iterm)
        return

      }

      guard !message.notificationResponse.hasStatus else {
        Logger.log(message: "notification response \(message.notificationResponse.status)", subsystem: .iterm)
        return
      }

      if message.notification.hasFocusChangedNotification {
        let focusChangedEvent = message.notification.focusChangedNotification
        Logger.log(message: "focus event! - \(focusChangedEvent.session)", subsystem: .iterm)

        let session = focusChangedEvent.session
        if session.count > 0 {
          self.sessionId = session
        }

        return
      }

    case .error(let err):
      Logger.log(message: "an error occurred - \(err)", subsystem: .iterm)
    }
  }

}

// swiftlint:disable:next type_name
class iTermEventStream {
  static func notificationRequest() -> Iterm2_ClientOriginatedMessage {
    var message = Iterm2_ClientOriginatedMessage()
    message.id = 0

    var notificationRequest = Iterm2_NotificationRequest()
    notificationRequest.session = "all"
    notificationRequest.subscribe = true
    notificationRequest.notificationType = .notifyOnFocusChange
    message.notificationRequest = notificationRequest

    return message
  }

}

extension iTermIntegration: UnixSocketDelegate {

  func socket(_ socket: UnixSocketClient, didReceive message: String) {
    Logger.log(message: "received message, '\(message)'", subsystem: .iterm)

    guard !message.contains("HTTP/1.1 401 Unauthorized") else {
      Logger.log(message: "disconnecting because connection refused", subsystem: .iterm)

      self.disconnect()
      return
    }

    if message.contains("HTTP/1.1 101 Switching Protocols") {
      Logger.log(message: "connection accepted!", subsystem: .iterm)
      self.isConnectedToAPI = true
      let message = iTermEventStream.notificationRequest()

      // swiftlint:disable:next force_try
      let payload = try! message.serializedData()
      let frame = ws.createWriteFrame(opcode: .binaryFrame,
                                      payload: payload,
                                      isCompressed: false)

      socket.send(data: frame)
    }

  }

  func socket(_ socket: UnixSocketClient, didReceive data: Data) {
    Logger.log(message: "received data", subsystem: .iterm)
    ws.add(data: data)
  }

  func socketDidClose(_ socket: UnixSocketClient) { }
}

extension iTermIntegration {
  func getCursorRect(in window: ExternalWindow) -> NSRect? {
    return Accessibility.getCursorRect()
  }

  func terminalIsFocused(in window: ExternalWindow) -> Bool {
    return true
  }
}

extension NSRunningApplication {

  static func forBundleId(_ bundleId: String?) -> NSRunningApplication? {
    guard let bundleId = bundleId else {
      return nil
    }

    return NSWorkspace.shared.runningApplications.filter({ return $0.bundleIdentifier == bundleId }).first

  }
}

extension NSWorkspace {
  func applicationIsInstalled(_ bundleId: String?) -> Bool {
    guard let bundleId = bundleId else {
      return false
    }

    return  NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) != nil
  }
}
