//
//  WindowService.swift
//  fig
//
//  Created by Matt Schrage on 6/28/20.
//  Copyright © 2020 Matt Schrage. All rights reserved.
//

import Foundation
import Cocoa

protocol WindowService {

  func topmostAllowlistedWindow() -> ExternalWindow?
  func topmostWindow(for app: NSRunningApplication) -> ExternalWindow?
  func previousFrontmostApplication() -> NSRunningApplication?
  func currentApplicationIsAllowlisted() -> Bool
  func allWindows(onScreen: Bool) -> [ExternalWindow]
  func allAllowlistedWindows(onScreen: Bool) -> [ExternalWindow]
  func previousAllowlistedWindow() -> ExternalWindow?
  func bringToFront(window: ExternalWindow)
  func takeFocus()
  func returnFocus()

  var isActivating: Bool { get }
  var isDeactivating: Bool { get }

}

class WindowServer: WindowService {
  var isActivating = false
  var isDeactivating = false

  func returnFocus() {
    if let app = self.previousApplication {
      self.isDeactivating = true
      if self.isActivating {
        NSWorkspace.shared.frontmostApplication?.activate(options: .activateIgnoringOtherApps)
      } else {
        app.activate(options: .activateIgnoringOtherApps)
      }
    }
  }

  @objc func didActivateApplication(notification: Notification) {
    if let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication {
      if app.isFig {
        self.isActivating = false
      } else if self.isDeactivating {
        self.isDeactivating = false
      }
    }
  }

  func takeFocus() {
    if NSWorkspace.shared.frontmostApplication?.isFig ?? false { return }
    if !self.isActivating {
      self.isActivating = true
      NSRunningApplication.current.activate(options: .activateIgnoringOtherApps)
    }
  }

  func bringToFront(window: ExternalWindow) {

    let appRef = AXUIElementCreateApplication(window.app.processIdentifier)
    var appWindows: CFArray?
    let error = AXUIElementCopyAttributeValues(appRef, kAXWindowsAttribute as CFString, 0, 99999, &appWindows)

    if error == .noValue || error == .attributeUnsupported {
      return
    }

    guard error == .success, let windows = appWindows as? [AXUIElement] else {
      return
    }

    let potentialTarget = windows.filter { PrivateWindow.getCGWindowID(fromRef: $0) == window.windowId}

    guard let target = potentialTarget.first else {
      return
    }

    AXUIElementPerformAction(target, kAXRaiseAction as CFString)

  }

  static let allowlistedWindowDidChangeNotification: NSNotification.Name = Notification.Name("allowlistedWindowDidChangeNotification")

  func previousAllowlistedWindow() -> ExternalWindow? {
    return self.previousWindow
  }

  func topmostAllowlistedWindow() -> ExternalWindow? {
    //        return AXWindowServer.shared.allowlistedWindow
    //        return self.allAllowlistedWindows(onScreen: true).first
    // fixed the workspace bug! Unfortunately it introduced a new bug when window becomes fullscreen + other weirdness
    guard self.currentApplicationIsAllowlisted() else { return nil }
    guard self.allAllowlistedWindows(onScreen: true).first != nil else { return nil }
    //        print("topmostAllowlistedWindow", self.allAllowlistedWindows(onScreen: true).first?.frame)
    //        print("topmostWindow", topmostWindow(for: NSWorkspace.shared.frontmostApplication!)?.frame)
    //        print("screen", NSScreen.main?.frame)
    return topmostWindow(for: NSWorkspace.shared.frontmostApplication!)
  }

  func currentApplicationIsAllowlisted() -> Bool {
    let allowlistedBundleIds = Integrations.allowlist
    if let app = NSWorkspace.shared.frontmostApplication,
       let bundleId = app.bundleIdentifier {
      //            print("currentAppBundleId = \(bundleId)")
      return allowlistedBundleIds.contains(bundleId)
    }

    return false
  }

  func allWindows(onScreen: Bool = false) -> [ExternalWindow] {
    let options: CGWindowListOption = onScreen ? CGWindowListOption.optionOnScreenOnly : .optionAll
    guard let rawWindows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
      return []
    }
    return rawWindows.compactMap { ExternalWindow(raw: $0) }
  }

  func allAllowlistedWindows(onScreen: Bool = false) -> [ExternalWindow] {
    return self.allWindows(onScreen: onScreen).filter { Integrations.allowlist.contains($0.bundleId ?? "") }
  }

  static let shared = WindowServer()

  func previousFrontmostApplication() -> NSRunningApplication? {
    return self.previousApplication
  }

  var previousApplication: NSRunningApplication?
  var previousWindow: ExternalWindow? {
    willSet(value) {
      if self.previousWindow != value {
        print("app: \(value?.bundleId ?? "<none>")")
        print("Old window \(self.previousWindow?.windowId ?? 0)")
        print("New window \(value?.windowId ?? 0)")
        NotificationCenter.default.post(name: WindowServer.allowlistedWindowDidChangeNotification, object: value)
      }
    }
  }

  init() {
    NSWorkspace.shared.notificationCenter.addObserver(
      self,
      selector: #selector(didActivateApplication(notification:)),
      name: NSWorkspace.didActivateApplicationNotification,
      object: nil
    )
    NSWorkspace.shared.notificationCenter.addObserver(
      self,
      selector: #selector(setPreviousApplication(notification:)),
      name: NSWorkspace.didDeactivateApplicationNotification,
      object: nil
    )
    NSWorkspace.shared.notificationCenter.addObserver(
      self,
      selector: #selector(spaceChanged),
      name: NSWorkspace.activeSpaceDidChangeNotification,
      object: nil
    )
    _ = Timer.scheduledTimer(
      timeInterval: 0.15,
      target: self,
      selector: #selector(setPreviousWindow),
      userInfo: nil,
      repeats: true
    )
  }
  //https://stackoverflow.com/questions/853833/how-can-my-app-detect-a-change-to-another-apps-window
  @objc func setPreviousWindow() {
    // don't set null when null
    if let window = AXWindowServer.shared.allowlistedWindow {// self.topmostAllowlistedWindow() {
      self.previousWindow = window
    }
  }

  @objc func spaceChanged() {
    // this is used to reset previous application when space is changed. Maybe should be nil.
    self.previousApplication = NSWorkspace.shared.frontmostApplication
  }

  @objc func setPreviousApplication(notification: NSNotification!) {
    self.previousApplication = notification!.userInfo![NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
  }

  func topmostWindow(for app: NSRunningApplication) -> ExternalWindow? {
    let appRef = AXUIElementCreateApplication(app.processIdentifier)
    var window: AnyObject?
    let result = AXUIElementCopyAttributeValue(appRef, kAXFocusedWindowAttribute as CFString, &window)

    if result == .apiDisabled {
      print("Accesibility needs to be enabled.")
      return nil
    }

    var position: AnyObject?
    var size: AnyObject?

    guard window != nil else {
      print("Window does not exist.")
      return nil
    }

    // swiftlint:disable force_cast
    let windowId = PrivateWindow.getCGWindowID(fromRef: window as! AXUIElement)
    // swiftlint:disable force_cast
    AXUIElementCopyAttributeValue(window as! AXUIElement, kAXPositionAttribute as CFString, &position)
    // swiftlint:disable force_cast
    AXUIElementCopyAttributeValue(window as! AXUIElement, kAXSizeAttribute as CFString, &size)

    if let position = position, let size = size {
      // swiftlint:disable force_cast
      let point = AXValueGetters.asCGPoint(value: position as! AXValue)
      // swiftlint:disable force_cast
      let bounds = AXValueGetters.asCGSize(value: size as! AXValue)

      //https://stackoverflow.com/a/19887161/926887
      let windowFrame = NSRect.init(x: point.x,
                                    y: NSScreen.screens[0].frame.maxY - point.y,
                                    width: bounds.width,
                                    height: bounds.height)
      // swiftlint:disable force_cast
      return ExternalWindow(windowFrame, windowId, app, (window as! AXUIElement))
    }
    return nil

  }
}

protocol App {
  var bundleIdentifier: String? { get }
  var localizedName: String? { get }
  var processIdentifier: pid_t { get }
}

extension ExternalApplication: App {
  var processIdentifier: pid_t {
    return self.pid
  }

  var bundleIdentifier: String? {
    return self.bundleId
  }
  var localizedName: String? {
    return self.title
  }
}
extension NSRunningApplication: App {}
typealias ExternalWindowHash = String

class ExternalWindow {
  let frame: NSRect
  let windowId: CGWindowID
  let windowLevel: CGWindowLevel?
  let app: App
  let accesibilityElement: AXUIElement?
  var windowMetadataService: WindowMetadataService = TerminalSessionLinker.shared// ShellHookManager.shared
  var lastTabId: String? {
    return windowMetadataService.getMostRecentFocusId(for: self.windowId)
  }

  var associatedShellContext: ShellContext? {
    return windowMetadataService.getAssociatedShellContext(for: self.windowId)
  }

  var associatedCommandContext: CommandContext? {
    return windowMetadataService.getAssociatedCommandContext(for: self.windowId)
  }

  var associatedEditBuffer: EditBuffer? {
    return windowMetadataService.getAssociatedEditBuffer(for: self.windowId)
  }

  var session: String? {
    return windowMetadataService.getTerminalSessionId(for: windowId)
  }

  init?(raw: [String: Any], accesibilityElement: AXUIElement? = nil) {
    guard let pid = raw["kCGWindowOwnerPID"] as? pid_t,
          let rect = raw["kCGWindowBounds"] as? [String: Any],
          let windowId = raw["kCGWindowNumber"] as? CGWindowID else {
      return nil
    }
    // swiftlint:disable identifier_name
    guard let x = rect["X"] as? CGFloat,
          // swiftlint:disable identifier_name
          let y = rect["Y"] as? CGFloat,
          let height = rect["Height"] as? CGFloat,
          let width = rect["Width"] as? CGFloat else {
      return nil
    }

    guard let app = NSRunningApplication(processIdentifier: pid) else {
      return nil
    }

    self.accesibilityElement = accesibilityElement
    self.windowLevel = raw["kCGWindowLayer"] as? CGWindowLevel
    self.app = app
    self.windowId = windowId
    self.frame = CGRect(x: x, y: y, width: width, height: height)
  }

  init?(backedBy axElementRef: AXUIElement, in app: ExternalApplication) {
    let windowId = PrivateWindow.getCGWindowID(fromRef: axElementRef)

    var position: AnyObject?
    var size: AnyObject?
    AXUIElementCopyAttributeValue(axElementRef, kAXPositionAttribute as CFString, &position)
    AXUIElementCopyAttributeValue(axElementRef, kAXSizeAttribute as CFString, &size)

    if let position = position, let size = size {
      // swiftlint:disable force_cast
      let point = AXValueGetters.asCGPoint(value: position as! AXValue)
      // swiftlint:disable force_cast
      let bounds = AXValueGetters.asCGSize(value: size as! AXValue)

      //https://stackoverflow.com/a/19887161/926887
      let windowFrame = NSRect(x: point.x,
                               y: NSMaxY(NSScreen.screens[0].frame) - point.y,
                               width: bounds.width,
                               height: bounds.height)

      self.frame = windowFrame
      self.windowId = windowId
      self.app = app
      self.windowLevel = ExternalWindow.getWindowLevel(for: windowId)
      self.accesibilityElement = axElementRef
    } else {
      return nil
    }

  }

  // This might be expensive, should be profiled
  static func getWindowLevel(for windowId: CGWindowID) -> CGWindowLevel? {
    guard let matchingWindow = (WindowServer.shared.allWindows().filter { $0.windowId == windowId }).first else {
      return nil
    }

    return matchingWindow.windowLevel
  }

  init(_ frame: NSRect, _ windowId: CGWindowID, _ app: App, _ accesibilityElement: AXUIElement? = nil) {
    self.frame = frame
    self.windowId = windowId
    self.app = app
    self.windowLevel = ExternalWindow.getWindowLevel(for: windowId)
    self.accesibilityElement = accesibilityElement

  }

  var frameWithoutTitleBar: NSRect {
    let titleBarHeight: CGFloat = 23.0

    return NSRect.init(x: frame.origin.x,
                       y: frame.origin.y - titleBarHeight,
                       width: frame.width,
                       height: frame.height - titleBarHeight)
  }

  var title: String? {
    return self.app.localizedName
  }

  var bundleId: String? {
    return self.app.bundleIdentifier
  }

  var hash: ExternalWindowHash {
    return self.windowMetadataService.getWindowHash(for: self.windowId)
  }

  var windowTitle: String? {
    guard let axref = self.accesibilityElement else { return nil }
    var title: AnyObject?
    let res = AXUIElementCopyAttributeValue(axref, kAXTitleAttribute as CFString, &title)

    guard res == .success else { return nil }

    return title as? String
  }

  var isFocusedTerminal: Bool {
    guard let provider = Integrations.providers[self.bundleId ?? ""] else {
      return false
    }

    return provider.terminalIsFocused(in: self)
  }

  var cursor: NSRect? {
    guard let provider = Integrations.providers[self.bundleId ?? ""] else {
      return nil
    }

    return provider.getCursorRect(in: self)
  }
  
  var isFullScreen: Bool? {
    guard let axref = self.accesibilityElement else { return nil }
    var isFullScreen: AnyObject?
    let res = AXUIElementCopyAttributeValue(axref, "AXFullScreen" as CFString, &isFullScreen)

    guard res == .success else { return nil }

    return isFullScreen as? Bool
    
  }
}

extension ExternalWindow: Hashable {
  func hash(into hasher: inout Hasher) {
    hasher.combine(self.windowId)
  }

  static func == (lhs: ExternalWindow, rhs: ExternalWindow) -> Bool {
    return lhs.windowId == rhs.windowId
  }
}

import FigAPIBindings
extension WindowServer {
  static func handleFocusRequest(_ request: Fig_WindowFocusRequest) throws -> Bool {
    switch request.type {
    case .takeFocus:
      WindowServer.shared.takeFocus()
    case .returnFocus:
      WindowServer.shared.returnFocus()
    default:
      throw APIError.generic(message: "Did not specify 'type'")
    }
    return true
  }
}
