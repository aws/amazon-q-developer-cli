//
//  Logging.swift
//  fig
//
//  Created by Matt Schrage on 5/27/20.
//  Copyright © 2020 Matt Schrage. All rights reserved.
//

import Foundation

protocol Logging {
  static func log(_ message: String)
}

class Logger {
  enum LogLevel: String {
    case error = "ERROR"
    case warn = "WARN"
    case info = "INFO"
    case debug = "DEBUG"
    case trace = "TRACE"
  }

  static var defaultLocation: URL = URL(fileURLWithPath: "\(NSHomeDirectory())/.fig/logs")

  enum Subsystem: String, CaseIterable {
    case global = "global"
    case telemetry = "telemetry"
    case windowServer = "windowserver"
    case keypress = "keypress"
    case xterm = "xterm"
    case javascript = "javascript"
    case tty = "tty-link"
    case iterm = "iterm"
    case docker = "docker"
    case ssh = "ssh"
    case pty = "pty"
    case cli = "cli"
    case shellhooks = "shellhooks"
    case windowEvents = "window-events"
    case buffer = "buffer"
    case autocomplete = "autocomplete"
    case cursor = "cursor"
    case xtermCursor = "xterm-cursor"
    case settings = "settings"
    case fish = "fish"
    case tmux = "tmux"
    case unix = "unix"
    case updater = "updater"
    case config = "config"
    case positioning = "positioning"
    case api = "api"
    case inputMethod = "input-method"
    case launchAgent = "launch-agent"
    case cPty = "c_pty"

    func pathToLogFile() -> URL {
      return Logger.defaultLocation
        .appendingPathComponent(self.rawValue, isDirectory: false)
        .appendingPathExtension("log")
    }

    func ansiColor() -> String {
      return Subsystem.colorOverridesTable[self] ?? Subsystem.colorTable[self]!
    }

    private static let colorOverridesTable: [Subsystem: String] =
      [ .autocomplete: "[36m", .xtermCursor: "[35;1m", .windowEvents: "[46;1m"
      ]

    private static let colorTable: [Subsystem: String] = {
      var table: [Subsystem: String] = [:]
      for subsystem in Subsystem.allCases {
        table[subsystem] = "[38;5;\((subsystem.rawValue.djb2hash % 256))m"

      }

      return table
    }()

    static let maxLength: Int = {
      // swiftlint:disable identifier_name
      return Subsystem.allCases.max { (a, b) -> Bool in
        return a.rawValue.count > b.rawValue.count
      }?.rawValue.count ?? 15
    }()
  }

  static func log(message: String,
                  priority: LogLevel = .debug,
                  subsystem: Subsystem = .global,
                  file: String = #file,
                  lineno: Int = #line) {
    var line = Logger.format(message, priority, subsystem, file, lineno)

    guard LocalState.canLogWithoutCrash else {
      return
    }

    if LocalState.shared.getValue(forKey: LocalState.loggingEnabledInternally) as? Bool ?? true {
      print(line)
    }

    guard let loggingEnabled = LocalState.shared.getValue(forKey: LocalState.logging) as? Bool, loggingEnabled else {
      return
    }

    if LocalState.shared.getValue(forKey: LocalState.colorfulLogging) as? Bool ?? true {
      line = Logger.format(message, priority, subsystem, file, lineno, colorful: true)
    }

    appendToLog(line, subsystem: subsystem)

  }

  static func resetLogs() {
    for system in Subsystem.allCases {
      try? FileManager.default.removeItem(atPath: system.pathToLogFile().path)
    }
    try? FileManager.default.createDirectory(at: Logger.defaultLocation,
                                             withIntermediateDirectories: true,
                                             attributes: nil)
    // Create all log files so that they can be tailed
    // even if no events have been logged yet
    for system in Subsystem.allCases {
      FileManager.default.createFile(atPath: system.pathToLogFile().path,
                                     contents: nil,
                                     attributes: nil)
    }
  }

  fileprivate static func appendToLog(_ line: String, subsystem: Subsystem = .global) {
    let filepath = subsystem.pathToLogFile()
    if let file = try? FileHandle(forUpdating: filepath) {
      file.seekToEndOfFile()

      file.write(line.data(using: .utf8)!)
      file.closeFile()
    } else {
      //            FileManager.default.createFile(atPath: filepath.absoluteString, contents: nil, attributes: nil)
      do {
        try line.write(to: filepath, atomically: true, encoding: String.Encoding.utf8)
      } catch {
        print("\(filepath.absoluteString) does not exist and could not be created. Logs will not be written.")
      }
    }
  }

  static func format(
    _ message: String,
    _ priority: LogLevel,
    _ subsystem: Subsystem,
    _ filepath: String,
    _ line: Int,
    colorful: Bool = false
  ) -> String {
    let timestamp = Logger.timestamp
    let filename = URL(fileURLWithPath: filepath).lastPathComponent

    var prefix = "\(subsystem.rawValue): "

    if colorful {
      prefix = "\u{001b}\(subsystem.ansiColor())" + prefix.trimmingCharacters(in: .whitespaces) + "\u{001b}[0m "
    }

    return [ timestamp,
             subsystem.rawValue + " [\(filename):\(line)]",
             priority.rawValue,
             message ].joined(separator: " | ") + "\n"
  }

  static var now: String {
    let now = Date()

    let formatter = DateFormatter()
    formatter.timeZone = TimeZone.current
    formatter.dateFormat = "yyyy-MM-dd HH:mm"

    return formatter.string(from: now)
  }

  static var timestamp: String {
    return String(format: "%.0f", Date().timeIntervalSince1970 * 1_000)
  }
}

fileprivate extension String {
  var djb2hash: Int {
    let unicodeScalars = self.unicodeScalars.map { $0.value }
    return unicodeScalars.reduce(5381) {
      ($0 << 5) &+ $0 &+ Int($1)
    }
  }

  //  var sdbmhash: Int {
  //      let unicodeScalars = self.unicodeScalars.map { $0.value }
  //      return unicodeScalars.reduce(0) {
  //          Int($1) &+ ($0 << 6) &+ ($0 << 16) - $0
  //      }
  //  }
}
