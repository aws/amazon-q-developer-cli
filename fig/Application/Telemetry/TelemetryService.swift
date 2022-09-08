//
//  TelemetryService.swift
//  fig
//
//  Created by Matt Schrage on 7/15/20.
//  Copyright © 2020 Matt Schrage. All rights reserved.
//

import Cocoa
import FigAPIBindings

enum LocalTelemetryEvent: String {
  case terminalUsage
  case keybufferEntered
  case showAutocompletePopup
  case insertViaAutocomplete
}

// Persists, aggregates and posts local telemetry events
protocol LocalTelemetryService {
  func store(event: LocalTelemetryEvent, with increment: Int, date: Date)
  func flush(eventsFor date: Date)
  func flushAll(includingCurrentDay: Bool)
  func register()
}

protocol TelemetryService {
  func obscure(_ input: String) -> String
  func track(
    event: TelemetryEvent,
    with payload: [String: String],
    completion: ((Data?, URLResponse?, Error?) -> Void)?
  )
}

enum TelemetryEvent: String {
  case ranCommand = "Ran CLI command"
  case selectedShortcut = "Selected a Shortcut"
  case viaJS = "Event via JS"
  case updatedApp = "Updated App"
  case promptedForAXPermission = "Prompted for AX Permission"
  case grantedAXPermission = "Granted AX Permission"
  case toggledAutocomplete = "Toggled Autocomplete"
  case toggledSidebar = "Toggled Sidebar"
  case quitApp = "Quit App"
  case viewDocs = "View Docs"
  case viewSupportForum = "View Support Forum"
  case joinSlack = "Join Slack"
  case sendFeedback = "Send Feedback"
  case dailyAggregates = "Aggregates"
  case firstTimeUser = "First Time User"
  case viaShell = "Event via Shell"
  case uninstallApp = "Uninstall App"
  case iTermSetup = "iTerm Setup"
  case launchedApp = "Launched App"
  case firstAutocompletePopup = "First Autocomplete Popup"
  case restartForOnboarding = "Restart for Shell Onboarding"
  case newWindowForOnboarding = "New Window for Shell Onboarding"
  case iTermSetupPrompted = "Prompted iTerm Setup"
  case showSecureInputEnabledAlert = "Show Secure Input Enabled Alert"
  case openSecureInputSupportPage = "Open Secure Input Support Page"
  case openedFigMenuIcon = "Opened Fig Menu Icon"
  case inviteAFriend = "Prompt to Invite"
  case runInstallationScript = "Running Installation Script"
  case telemetryToggled = "Toggled Telemetry"
  case openedSettingsPage = "Opened Settings Page"

}

class TelemetryProvider: TelemetryService {
  static let shared = TelemetryProvider(defaults: Defaults.shared)

  private var defaults: Defaults

  private var terminalObserver: TerminalUsageObserver?

  private var deviceId: String? {
    let platformExpert = IOServiceGetMatchingService(kIOMasterPortDefault, IOServiceMatching("IOPlatformExpertDevice") )

    guard platformExpert > 0 else {
      return nil
    }

    let deviceId = IORegistryEntryCreateCFProperty(platformExpert, kIOPlatformUUIDKey as CFString, kCFAllocatorDefault, 0).takeUnretainedValue() as? String

    IOObjectRelease(platformExpert)

    return deviceId
  }

  init(defaults: Defaults) {
    self.defaults = defaults
  }

  func obscure(_ input: String) -> String {
    return String(input.map { $0.isLetter ? "x" : $0 }.map { $0.isNumber ? "0" : $0 })
  }

  func page(
    category: String,
    name: String,
    with properties: [String: Any]
  ) {
    var body: [String: Any] = [:]

    body["properties"] = addDefaultProperties(to: properties)
    body["category"] = category
    body["name"] = name

    body["anonymousId"] = LocalState.shared.anonymousId

    guard !defaults.telemetryDisabled else {

      Logger.log(message: "not sending page event because telemetry is disabled")
      return
    }

    upload(to: "page", with: body, completion: nil)
  }

  func track(
    event: TelemetryEvent,
    with properties: [String: String],
    completion: ((Data?, URLResponse?, Error?) -> Void)? = nil
  ) {
    track(event: event.rawValue, with: properties, completion: completion)
  }

  func track(
    event: String,
    with properties: [String: Any],
    completion: ((Data?, URLResponse?, Error?) -> Void)? = nil
  ) {
    var body: [String: Any] = [:]

    body["properties"] = addDefaultProperties(to: properties)
    body["event"] = event
    body["anonymousId"] = LocalState.shared.anonymousId
    body["useUnprefixed"] = true

    if defaults.telemetryDisabled {
      let eventsToSendEvenWhenDisabled: [TelemetryEvent] = [.telemetryToggled]
      let sendEvent = eventsToSendEvenWhenDisabled.contains { (allowlistedEvent) -> Bool in
        return allowlistedEvent.rawValue == event
      }

      guard sendEvent else {
        print("telemetry: not sending event because telemetry is disabled")
        completion?(nil, nil, nil)
        return
      }

    }

    // Ensure old uuid is aliased before making request.
    Defaults.shared.migrateUUID()

    upload(to: "track", with: body, completion: completion)
  }

  func identify(
    with traits: [String: Any],
    needsPrefix prefix: String? = "trait_",
    shouldIgnoreTelemetryPreferences: Bool = false
  ) {
    var body: [String: Any] = [:]

    body["traits"] = traits
    body["useUnprefixed"] = true
    body["anonymousId"] = LocalState.shared.anonymousId

    if defaults.telemetryDisabled && !shouldIgnoreTelemetryPreferences {
      print("telemetry: not sending identification event because telemetry is disabled")
      return
    }

    // Ensure old uuid is aliased before making request.
    Defaults.shared.migrateUUID()

    upload(to: "identify", with: body)
  }

  func alias(userId: String?) {

    if defaults.telemetryDisabled {
      print("telemetry: not sending identification event because telemetry is disabled")
      return
    }

    // Ensure old uuid is aliased before making request.
    Defaults.shared.migrateUUID()

    upload(to: "alias", with: [
      "previousId": LocalState.shared.anonymousId,
      "userId": userId ?? ""
    ])
  }

  func upload(
    to endpoint: String,
    with body: [String: Any],
    completion: ((Data?, URLResponse?, Error?) -> Void)? = nil
  ) {
    guard let json = try? JSONSerialization.data(withJSONObject: body, options: .sortedKeys) else { return }
    print(json)

    let url = Remote.API
        .appendingPathComponent("telemetry")
        .appendingPathComponent(endpoint)

    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.httpBody = json
    request.setValue("application/json; charset=utf-8", forHTTPHeaderField: "Content-Type")

    DispatchQueue.global(qos: .background).async {

      try? Credentials.shared.authorizeRequest(request: &request)

      let task = URLSession.shared.dataTask(with: request) { (data, res, err) in
        if let handler = completion {
          handler(data, res, err)
        }
      }

      task.resume()
    }

  }

  fileprivate func addPrefixToKeys(prefix: String, dict: [String: String]) -> [String: String] {

    return dict.reduce([:]) { (out, pair) -> [String: String] in
      var new = out
      let (key, value) = pair
      new["\(prefix)\(key)"] = value
      return new
    }
  }

  fileprivate func addDefaultProperties(
    to properties: [String: Any]
  ) -> [String: Any] {
    let email = defaults.email ?? ""
    let domain = String(email.split(separator: "@").last ?? "unregistered")

    var defaultsProperties = [
      "event_origination_source": "desktop",
      "device_os": "macos",
      "device_arch": Diagnostic.arch,
      "device_install_method": "unknown",
      "device_macos_release_version": Diagnostic.osReleaseVersion,
      "desktop_version": defaults.version,
      "desktop_legacy_build": Diagnostic.build,

      // todo(mschrage): legacy fields to be removed
      "domain": domain,
      "email": email,
      "version": defaults.version,
      "build": Diagnostic.build,
      "os": Diagnostic.osReleaseVersion
    ]

    if let deviceId = deviceId {
      defaultsProperties["device_id"] = deviceId
    }

    return properties.merging(defaultsProperties) { $1 }
  }
}

extension TelemetryProvider: LocalTelemetryService {
  func register() {
    self.terminalObserver = TerminalUsageObserver()

    NotificationCenter.default.addObserver(
      self,
      selector: #selector(calendarDayDidChange),
      name: .NSCalendarDayChanged,
      object: nil
    )
    // flush previous events
    flushAll()

    // register other telemetry observers!
    NotificationCenter.default.addObserver(self,
                                           selector: #selector(lineAcceptedInKeystrokeBuffer),
                                           name: FigTerm.lineAcceptedInKeystrokeBufferNotification,
                                           object: nil)
    NotificationCenter.default.addObserver(self,
                                           selector: #selector(insertionInTerminal),
                                           name: FigTerm.insertedTextNotification,
                                           object: nil)
    NotificationCenter.default.addObserver(self,
                                           selector: #selector(showAutocompletePopup),
                                           name: NSNotification.Name("showAutocompletePopup"),
                                           object: nil)
    NotificationCenter.default.addObserver(self,
                                           selector: #selector(logTerminalUsage(_:)),
                                           name: TerminalUsageObserver.terminalApplicationLostFocusNotification,
                                           object: nil)
  }

  @objc fileprivate func calendarDayDidChange() {
    Logger.log(message: "Calendar Day changed")
    self.flushAll()
  }

  // Local Telemetry Observers
  @objc fileprivate func lineAcceptedInKeystrokeBuffer() {
    Logger.log(message: "lineAcceptedInKeystrokeBuffer")
    self.store(event: .keybufferEntered)
  }

  @objc fileprivate func insertionInTerminal() {
    Logger.log(message: "insertionInTerminal")
    self.store(event: .insertViaAutocomplete)
  }

  @objc fileprivate func showAutocompletePopup() {
    Logger.log(message: "showAutocompletePopup")
    self.store(event: .showAutocompletePopup)

    if !defaults.hasShownAutocompletePopover {
      defaults.hasShownAutocompletePopover = true
      track(event: .firstAutocompletePopup, with: [:])
    }
  }

  @objc fileprivate func logTerminalUsage(_ notification: Notification) {
    Logger.log(message: "logTerminalUsage")
    if let time = notification.object as? TimeInterval {
      self.store(event: .terminalUsage, with: Int(time))
    }
  }

  func flushAll(includingCurrentDay: Bool = false) {
    let today = Date(timeIntervalSinceNow: 0).telemetryDayIdentifier
    self.pending.forEach {
      // exclude current day unless explicitly pushing all events
      if includingCurrentDay || $0 != today {
        self.flush(eventsFor: $0)
      }
    }
  }

  var pending: Set<TelemetryUTCDate> {
    return  Set(UserDefaults.standard.stringArray(forKey: "pendingTelemetryUpload") ?? [])
  }

  func store(event: LocalTelemetryEvent, with increment: Int = 1, date: Date = Date(timeIntervalSinceNow: 0)) {
    DispatchQueue.global(qos: .utility).async {
      let dateIdentifier = date.telemetryDayIdentifier
      let key = "\(dateIdentifier)#\(event.rawValue)"
      let aggregate = UserDefaults.standard.integer(forKey: key)
      UserDefaults.standard.set(aggregate + increment, forKey: key)

      // update what dates have data to send
      var pending: Set<String> = Set(UserDefaults.standard.stringArray(forKey: "pendingTelemetryUpload") ?? [])
      Logger.log(message: pending.joined(separator: ","))

      pending.insert(dateIdentifier)
      UserDefaults.standard.set(Array(pending), forKey: "pendingTelemetryUpload")
    }
  }

  // send logged & aggregated events to server
  fileprivate func flush (eventsFor dateIdentifier: TelemetryUTCDate) {
    let aggregatableEvents: Set<LocalTelemetryEvent> = [
      .insertViaAutocomplete,
      .keybufferEntered,
      .showAutocompletePopup,
      .terminalUsage
    ]
    var keys: Set<String> = []
    let countsForDate = aggregatableEvents.map { (event) -> (LocalTelemetryEvent, Int) in
      let key = "\(dateIdentifier)#\(event.rawValue)"
      keys.insert(key)
      let total = UserDefaults.standard.integer(forKey: key)
      return (event, total)
    }
    var payload: [String: String] = countsForDate.reduce(into: [:], { (dict, pair) in
      let (event, count) = pair
      dict[event.rawValue] = "\(count)"
    })
    payload["date"] = dateIdentifier
    payload["telemetryDisabled"] = UserDefaults.standard.bool(forKey: "\(dateIdentifier)#telemetryDisabled")
      ? "true"
      : "false"
    print("aggregate:", countsForDate)
    // todo: add completion handler for success and failure
    // clean cache on success
    // reschedule on failure
    self.track(event: .dailyAggregates, with: payload
    ) { (_, _, error) in
      guard error == nil else {
        // Don't delete cached data, try to send later
        Logger.log(message: "Failed to flush events with error:\(error!.localizedDescription)")
        return
      }

      // delete cached data
      keys.forEach {
        Logger.log(message: "Delete telemetry for key: \($0)")
        UserDefaults.standard.removeObject(forKey: $0)
      }

      // remove date from [pendingUpload] store
      if let pending = UserDefaults.standard.stringArray(forKey: "pendingTelemetryUpload") {
        let filtered = pending.filter { $0 != dateIdentifier}
        UserDefaults.standard.set(filtered, forKey: "pendingTelemetryUpload")
      }
    }
  }

  func flush(eventsFor date: Date) {
    let dateIdentifier = date.telemetryDayIdentifier
    self.flush(eventsFor: dateIdentifier)
  }
}

typealias TelemetryUTCDate = String
extension Date {
  var telemetryDayIdentifier: TelemetryUTCDate {
    let cal: Calendar = Calendar(identifier: .gregorian)
    let fmt = DateFormatter()
    fmt.dateFormat = " yyyy-MM-dd'T'HH:mm:ssZ"
    return fmt.string(from: cal.startOfDay(for: self))
  }
}

extension TelemetryProvider {
  @discardableResult
  func handleAliasRequest(_ request: Fig_TelemetryAliasRequest) throws -> Bool {
    guard request.hasUserID else {
      throw APIError.generic(message: "No user id specified.")
    }

    alias(userId: request.userID)

    return true
  }

  @discardableResult
  func handleTrackRequest(_ request: Fig_TelemetryTrackRequest) throws -> Bool {
    guard request.hasEvent else {
      throw APIError.generic(message: "No event specified.")
    }

    let keys = request.properties.map { $0.key }
    let values = request.properties.map { $0.value }
    let payload = Dictionary(uniqueKeysWithValues: zip(keys, values))

    track(event: request.event, with: payload)

    return true
  }

  @discardableResult
  func handleIdentifyRequest(_ request: Fig_TelemetryIdentifyRequest) throws -> Bool {

    let keys = request.traits.map { $0.key }
    let values = request.traits.map { $0.value }
    let payload = Dictionary(uniqueKeysWithValues: zip(keys, values))

    identify(with: payload)

    return true
  }

  @discardableResult
  func handlePageRequest(_ request: Fig_TelemetryPageRequest) throws -> Bool {
    guard let json = request.jsonBlob.parseAsJSON(), request.hasJsonBlob else {
      throw APIError.generic(message: "Properties dictionary was not passed or was not valid JSON.")
    }

    page(category: request.category, name: request.name, with: json)

    return true
  }
}
