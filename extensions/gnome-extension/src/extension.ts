import Clutter from "gi://Clutter";
import Gio from "gi://Gio";
import GLib from "gi://GLib";
import Meta from "gi://Meta";
import St from "gi://St";

import { Extension } from "resource:///org/gnome/shell/extensions/extension.js";
import * as PanelMenu from "resource:///org/gnome/shell/ui/panelMenu.js";

declare let DEBUG: boolean;

const LOG_PREFIX = "Amazon Q GNOME Integration:";

/**
 * @param {any[]} messages
 * @returns {void}
 */
function log_msg(...messages) {
  return console.log(LOG_PREFIX, ...messages);
}

/**
 * @param {any[]} messages
 * @returns {void}
 */
function debug(...messages) {
  if (DEBUG) {
    return console.log(LOG_PREFIX, ...messages);
  }
}

const RESOURCES_PREFIX =
  "resource:///org/gnome/shell/extensions/amazon-q-for-cli-gnome-integration";

/**
 * @template {string[]} T
 * @param  {T} resources
 * @returns {T extends [string, string, ...string[]] ? string[] : string}
 */
function resource(...resources) {
  if (resources.length >= 2) return resources.map((path) => resource(path));
  else if (resources.length === 1) return `${RESOURCES_PREFIX}/${resources[0]}`;
  else if (DEBUG)
    throw new RangeError("Expected one or more resources, got zero");
  else return null;
}

/**
 * Returns the path to the desktop socket.
 *
 * @private
 * @function
 * @returns {string} The path to the desktop socket.
 */
function socket_address() {
  return `${GLib.getenv("XDG_RUNTIME_DIR")}/cwrun/desktop.sock`;
}

/**
 * Converts a message to the format that the desktop socket expects.
 *
 * @private
 * @function
 * @param {string} hook The hook that the payload is for.
 * @param {object} payload The payload of the message.
 * @returns {Uint8Array} The converted message.
 */
function socket_encode(hook, payload) {
  const header = "\x1b@fig-json\x00\x00\x00\x00\x00\x00\x00\x00";
  const body = JSON.stringify({ hook: { [hook]: payload } });

  const message = new TextEncoder().encode(header + body);

  // I'd use a Uint32Array pointing to the same buffer to do this, but the
  // length part of the header is misaligned by two bytes...
  let length = body.length << 0;
  for (let i = 0; i < 4; i++) {
    const byte = length & 0xff;
    message[header.length - i - 1] = byte;
    length = (length - byte) / 256;
  }

  return message;
}

/**
 * Creates a `PromiseLike<T>` that can be cancelled by calling its `cancel`
 * method.
 *
 * ### Example
 *
 * ```js
 * const my_non_cancellable_promise = sleep(5000).then(() => 123);
 *
 * const my_cancellable_promise = (() => {
 *   let cancelled = false;
 *   return cancellable(
 *     new Promise((resolve) => sleep(3000).then(() => !cancelled && resolve(456))),
 *     () => cancelled = true,
 *   );
 * })();
 *
 * my_cancellable_promise.cancel();
 *
 * Promise.race([ my_non_cancellable_promise, my_cancellable_promise ]).then((result) => {
 *   // will always log 123, despite my_non_cancellable_promise sleeping for
 *   // longer, because my_cancellable_promise will never resolve since it was
 *   // cancelled.
 *   console.log(result);
 * });
 * ```
 *
 * @private
 * @function
 * @template T
 * @param {PromiseLike<T>} promise A promise that is guaranteed by the caller to
 * stop execution when `cancel` is called.
 * @param {() => void} cancel A function that will be called when the returned
 * objects `cancel` method is called.
 * @returns {PromiseLike<T> & { promise: PromiseLike<T>, cancel: () => void }}
 * A `PromiseLike<T>` that can be cancelled.
 */
function cancellable(promise, cancel) {
  return Object.freeze(
    Object.assign(Object.create(null), {
      promise,
      cancel,

      then(onresolve, onreject) {
        return cancellable(promise.then(onresolve, onreject), cancel);
      },

      catch(onreject) {
        return cancellable(promise.catch(onreject), cancel);
      },
    }),
  );
}

/**
 * Returns one or more numbers as strings, with their ordinal suffixes.
 *
 * ### Example
 *
 * ```js
 * console.log(ordinal(1, 2, 3, 4, 5)); // logs "[ '1st', '2nd', '3rd', '4th', '5th' ]"
 * ```
 *
 * @private
 * @function
 * @template {number|number[]} N
 * @param {N} numbers The number(s) to format.
 * @returns {N extends [number, number, ...number[]] ? string[] : string} The formatted number(s).
 */
function ordinal(...numbers) {
  if (numbers.length === 1) {
    const number = numbers[0];

    const suffixes = {
      1: "st",
      2: "nd", // spellchecker:disable-line
      3: "rd",
    };

    if (number % 100 < 20) return `${number}${suffixes[number % 100] ?? "th"}`;
    else return `${number}${suffixes[number % 10] ?? "th"}`;
  } else {
    return numbers.map(ordinal);
  }
}

/**
 * The main class for managing the extensions state.
 */
export default class QCliExtension extends Extension {
  /** @public @property @type {boolean} */
  get connected() {
    return this.#connected;
  }

  /**
   * Whether or not the extension is connected to the desktop socket.
   *
   * @private @property @type {boolean}
   */
  #connected = false;

  /**
   * The binding object between the extension's `connected` property and the
   * panel icon's `connected` property. This enables automatically updating
   * the panel icon's connected state whenever the extension's connected
   * property changes.
   *
   * @private @property @type {import("../types/.gobject").Binding|null}
   */
  #connected_binding = null;

  /**
   * Whether or not the extension is currently connecting to the desktop
   * socket and to mutter.
   *
   * @private @property @type {boolean}
   */
  #connecting = false;

  /**
   * A map of GObject to connection id's. This is used by the extension to keep track
   * of which relevant objects (global display, extension settings, etc.) we connect to,
   * and disconnect when disconnecting from the desktop socket and mutter.
   *
   * @private @property @type {Map<import("../types/.gobject").Object, Set<number>>}
   */
  #connections = new Map();

  /**
   * Whether or not the extension is currently disconnecting from the desktop socket
   * and mutter.
   *
   * @private @property @type {boolean}
   */
  #disconnecting = false;

  /**
   * The widget representing the Amazon Q icon in the panel.
   *
   * @private @property @type {PanelIcon|null}
   */
  #panel_icon = null;

  /** @private @property @type {import("../types/.gio").Resource|null} */
  #resources = null;

  /**
   * The extension settings, set according to the schema defined under
   * [../schemas/org.gnome.shell.extensions.amazon-q-for-cli-gnome-integration.gschema.xml]
   *
   * @private @property @type {import("../types/.gio").Settings|null}
   */
  #settings = null;

  /**
   * The desktop socket.
   *
   * @private @property @type {import("../types/.gio").Socket|null}
   */
  #socket = null;

  /** @private @property @type {Queue} */
  #queue = new Queue();

  /**
   * The window currently in focus.
   *
   * @private @property @type {import("../types/.gobject").Object|null} */
  #window = null;

  /**
   * The GLib Source containing a callback that resolves a promise. Used
   * to implement sleeping for some set time.
   *
   * TODO: Why is this used instead of setTimeout/clearTimeout, which
   * are supported since v41: https://gjs.guide/guides/gjs/intro.html#web-apis
   *
   * @private @property @type {import("../types/.glib").Source|null} */
  #sleep_source = null;

  /**
   * The GLib Source containing a callback that attempts to connect to the
   * desktop socket. Tracking this ensures we can destroy it when the
   * extension is disabled.
   *
   * @private @property @type {import("../types/.glib").Source|null} */
  #retry_source = null;

  /**
   * Enables the extension, starting to connect to the Desktop socket and mutter
   * quietly in the background.
   *
   * @public @method @returns {void}
   */
  enable() {
    debug("Enabling the extension");

    // Load and register resource files.
    this.#resources = Gio.Resource.load(
      `${this.metadata.path}/resources/amazon-q-for-cli-gnome-integration.gresource`,
    );
    Gio.resources_register(this.#resources);

    // Get the settings object.
    this.#settings = this.getSettings();

    // Watch for the user changing the "show-panel-icon" preference.
    // this.#connect_to_object(this.#settings, "changed::show-panel-icon", () => {
    //   if (this.#settings.get_boolean("show-panel-icon")) {
    //     // If the panel icon doesn't exist, create it and bind the connected
    //     // property, then add it to the panel.
    //     if (this.#panel_icon === null) {
    //       this.#panel_icon = new PanelIcon({
    //         connected: this.#connected,
    //       });
    //       this.#connected_binding = this.bind_property(
    //         "connected",
    //         this.#panel_icon,
    //         "connected",
    //         GObject.BindingFlags.DEFAULT,
    //       );
    //     }
    //     Main.panel.addToStatusArea(
    //       "AmazonQForCLI",
    //       this.#panel_icon,
    //       0,
    //       "right",
    //     );
    //     // If the panel icon exists, destroy it.
    //   } else if (this.#panel_icon !== null) {
    //     this.#connected_binding.unbind();
    //     this.#connected_binding = null;
    //     this.#panel_icon.destroy();
    //     this.#panel_icon = null;
    //   }
    // });

    // if (this.#settings.get_boolean("show-panel-icon")) {
    //   this.#panel_icon = new PanelIcon({
    //     connected: this.#connected,
    //   });
    //   this.#connected_binding = this.bind_property(
    //     "connected",
    //     this.#panel_icon,
    //     "connected",
    //     GObject.BindingFlags.DEFAULT,
    //   );
    //   Main.panel.addToStatusArea("AmazonQForCLI", this.#panel_icon, 0, "right");
    // }

    this.#connect();
  }

  /**
   * Disables the extension. Note that this waits for the extension to finish
   * becoming enabled if it is in the process of doing so. This prevents the
   * extension from crashing if the user spams the extension enable/disable
   * switch.
   *
   * @public @method @returns {void}
   */
  disable() {
    debug("Disabling the extension");

    // Unregister the resource files.
    Gio.resources_unregister(this.#resources);
    this.#resources = null;

    // Disconnect from and delete the settings object.
    this.#disconnect_from_object(this.#settings, null);
    this.#settings = null;

    // If the panel icon exists, destroy it.
    if (this.#panel_icon !== null) {
      this.#connected_binding.unbind();
      this.#connected_binding = null;
      this.#panel_icon.destroy();
      this.#panel_icon = null;
    }

    this.#disconnect();

    if (this.#sleep_source !== null && !this.#sleep_source.is_destroyed()) {
      this.#sleep_source.destroy();
      this.#sleep_source = null;
    }
    if (this.#retry_source !== null && !this.#retry_source.is_destroyed()) {
      this.#retry_source.destroy();
      this.#retry_source = null;
    }
  }

  /**
   * Returns a promise that will resolve after roughly the specified amount of
   * milliseconds.
   *
   * @private
   * @function
   * @param {number} millis
   * @returns {PromiseLike<void>}
   */
  #sleep(millis) {
    let cancelled = false;
    return cancellable(
      new Promise<void>((resolve) => {
        const source = GLib.timeout_source_new(millis);
        source.set_callback(() => {
          if (!cancelled) resolve();
          return false;
        });
        source.attach(null);
        this.#sleep_source = source;
      }),
      () => {
        cancelled = true;
      },
    );
  }

  #connect() {
    if (this.#connecting) {
      debug("#connect: Ignoring connect call since already connecting.");
      return;
    }

    this.#connecting = true;
    this.#disconnecting = false;

    this.#queue.push(
      new Queue.Item(() =>
        this.#sleep(100)
          .then(() => this.#connect_to_socket())
          .then(() => this.#connect_to_mutter())
          .finally(() => {
            this.#connecting = false;
          }),
      ),
    );
  }

  /**
   * Connects to all of the signals that this extension uses from mutter.
   *
   * The connection ids are stored in a map so that they may be disconnected
   * later to ensure garbage collection.
   *
   * @returns {void}
   */
  #connect_to_mutter() {
    log_msg("Connecting to mutter...");

    this.#window = global.display.focus_window;

    this.#connect_to_object(this.#window, "size-changed", () => {
      debug("mutter: size-changed");
      return this.#send_window_data(null);
    });

    // Subscribe to receive updates when the global `MetaDisplay` "focus-window"
    // property changes.
    this.#connect_to_object(global.display, "notify::focus-window", () => {
      debug(
        `mutter: notify::focus-window on ${global.display.focus_window.get_wm_class()}`,
      );
      if (this.#window !== global.display.focus_window) {
        this.#disconnect_from_object(this.#window, null);

        this.#window = global.display.focus_window;

        this.#connect_to_object(this.#window, "size-changed", () =>
          this.#send_window_data(null),
        );

        this.#send_window_data(null);
      }
    });

    // Subscribe to receive updates when the overlay key is pressed
    this.#connect_to_object(global.display, "overlay-key", () => {
      debug("mutter: overlay-key");
      this.#send_window_data(true);
    });

    // Subscribe to be notified when a new grab operation begins.
    // This is needed because neither GNOME shell or mutter expose a signal that
    // is fired when a `MetaWindow` is moved. So, the solution is to subscribe
    // to the display when a grab operation starts; AKA when the user starts
    // moving around a window, and then updating the window data whenever the
    // cursor moves until the grab operation ends.
    this.#connect_to_object(
      global.display,
      "grab-op-begin",
      (_, __, grab_op) => {
        if (
          grab_op === Meta.GrabOp.MOVING ||
          grab_op === Meta.GrabOp.KEYBOARD_MOVING
        ) {
          if (this.#window !== global.display.focus_window) {
            this.#disconnect_from_object(this.#window, null);

            this.#window = global.display.focus_window;

            this.#connect_to_object(this.#window, "size-changed", () =>
              this.#send_window_data(null),
            );
          }

          this.#send_window_data(null);

          const cursor = Meta.CursorTracker.get_for_display(global.display);
          const cursor_connection = this.#connect_to_object(
            cursor,
            "position-invalidated",
            () => this.#send_window_data(null),
          );

          const display_connection = this.#connect_to_object(
            global.display,
            "grab-op-end",
            () => {
              this.#disconnect_from_object(global.display, display_connection);
              this.#disconnect_from_object(cursor, cursor_connection);
            },
          );
        }
      },
    );

    log_msg("Connected to mutter!");

    return Promise.resolve();
  }

  /**
   * Connects to `signal` on `object`, storing a relation between the two so
   * that it can be disconnected automatically if the extension is disabled.
   *
   * @param {import("../types/.gobject").Object} object
   * @param {string} signal
   * @param {() => void} handler
   * @returns {number}
   */
  #connect_to_object(object, signal, handler) {
    if (object === null) return;
    const connections = this.#connections.get(object) ?? new Set();
    const connection = object.connect(signal, handler);
    connections.add(connection);
    this.#connections.set(object, connections);
    return connection;
  }

  /**
   * Repeatedly tries to connect to the Desktop socket, ignoring errors, until it
   * either successfully connects or is cancelled.
   *
   * @returns {Promise<void> & { cancel: () => void, promise: Promise<void> }}
   */
  #connect_to_socket() {
    const client = Gio.SocketClient.new();
    const socket_path = socket_address();
    const address = Gio.UnixSocketAddress.new(socket_path);
    const cancel = Gio.Cancellable.new();

    let backoff_ms = 1000;
    let attempts = 0;
    let finished = false;

    return cancellable(
      // TODO: this shouldn't take any
      new Promise<void>((resolve) => {
        const attempt = () => {
          if (finished) return;

          attempts++;

          debug(
            `Connecting to socket at: "${socket_path}" (${ordinal(attempts)} try)...`,
          );

          client.connect_async(address, cancel, (socket_client, result) => {
            if (finished) return;

            try {
              this.#socket = socket_client.connect_finish(result).get_socket();

              this.#connected = true;
              // this.notify("connected");

              resolve();

              log_msg("Connected to socket!");
            } catch (error) {
              // 32 times until it maxes out at 20 seconds.
              backoff_ms = Math.min(backoff_ms * 1.1, 20_000);

              log_msg(
                `Encountered an error while connecting to socket at "${socket_path}" (${ordinal(attempts)} try).` +
                  `Retrying after ${Math.round(backoff_ms / 1000)} seconds. Reason: ${error}`,
              );

              const source = GLib.timeout_source_new(backoff_ms);
              source.set_priority(GLib.PRIORITY_LOW);
              source.set_callback(() => {
                attempt();
                return false;
              });
              source.attach(GLib.MainContext.default());
              this.#retry_source = source;
            }
          });
        };

        attempt();
      }),
      () => {
        if (finished) return;
        log_msg("Cancelling connection to socket...");
        finished = true;
        cancel.cancel();
      },
    );
  }

  #disconnect() {
    if (this.#disconnecting) {
      debug(
        "#disconnect: Ignoring disconnect call since already disconnecting",
      );
      return;
    }

    this.#disconnecting = true;
    this.#connecting = false;

    this.#queue.push(
      new Queue.Item(() =>
        this.#sleep(100)
          .then(() => this.#disconnect_from_objects())
          .then(() => this.#disconnect_from_socket())
          .finally(() => {
            this.#disconnecting = false;
          }),
      ),
    );
  }

  /**
   * @param {import("../types/.gobject").Object} object
   * @param {number?} connection
   * @returns {boolean}
   */
  #disconnect_from_object(object, connection) {
    if (object === null) return;

    const connections = this.#connections.get(object) ?? new Set();

    if (connection !== null) {
      object.disconnect(connection);

      const removed = connections.delete(connection);

      // We're not adding connections, so the only change in size could be
      // negative. As such, we only need to check if the set is now empty and
      // delete it if it is to ensure garbage collection.
      if (connections.size === 0) this.#connections.delete(object);

      return removed;
    } else {
      for (const to_disconnect of connections) object.disconnect(to_disconnect);
      return this.#connections.delete(object);
    }
  }

  #disconnect_from_objects() {
    try {
      log_msg("Disconnecting from objects...");

      this.#window = null;

      for (const [object, connections] of this.#connections) {
        for (const connection of connections) object.disconnect(connection);
        connections.clear();
      }
      this.#connections.clear();

      log_msg("Disconnected from objects.");

      return Promise.resolve();
    } catch (error) {
      return Promise.reject(error);
    }
  }

  #disconnect_from_socket() {
    try {
      if (this.#socket === null) return Promise.resolve();

      log_msg("Disconnecting from socket...");

      this.#socket.close();
      this.#socket = null;

      this.#connected = false;
      // this.notify("connected");

      log_msg("Disconnected from socket.");

      return Promise.resolve();
    } catch (error) {
      return Promise.reject(error);
    }
  }

  /**
   * @param {boolean} overlay_pressed
   */
  #send_window_data(overlay_pressed) {
    // Mutter populates wm class/instance with the app_id on Wayland.
    const wm_class = this.#window.get_wm_class();
    // https://mutter.gnome.org/meta/method.Window.get_frame_rect.html
    const inner_rect = this.#window.get_frame_rect();
    // https://mutter.gnome.org/meta/method.Window.get_buffer_rect.html
    const outer_rect = this.#window.get_buffer_rect();
    const scale = global.display.get_monitor_scale(this.#window.get_monitor());

    debug(
      `Sending data for rect inner ${inner_rect.x},${inner_rect.y},${inner_rect.width},${inner_rect.height}` +
        ` outer ${outer_rect.x},${outer_rect.y},${outer_rect.width},${outer_rect.height} with scale ${scale}`,
    );

    try {
      this.#socket.send(
        socket_encode("focusedWindowData", {
          source: "gse",
          id: wm_class,
          inner: {
            x: inner_rect.x,
            y: inner_rect.y,
            width: inner_rect.width,
            height: inner_rect.height,
          },
          outer: {
            x: outer_rect.x,
            y: outer_rect.y,
            width: outer_rect.width,
            height: outer_rect.height,
          },
          hide: overlay_pressed,
          scale,
        }),
        null,
      );
    } catch {
      log_msg("Failed to send a message to the socket, disconnecting.");

      this.#disconnect();
      this.#connect();
    }
  }
}

/** @public @class PanelIcon */
// eslint-disable-next-line @typescript-eslint/no-unused-vars
class PanelIcon extends PanelMenu.Button {
  /** @public @property @type {boolean} */
  get connected() {
    return this.#connected;
  }

  /** @public @property @type {boolean} */
  set connected(value) {
    this.#connected = value;
    this.notify("connected");
  }

  /** @private @property @type {boolean} */
  #connected;
  /** @private @property @type {number} */
  #connection;
  /** @private @property @type {import("../types/.st").Icon} */
  #icon;
  /** @private @property @type {import("../types/.gio").Icon} */
  #icon_connected;
  /** @private @property @type {import("../types/.gio").Icon} */
  #icon_disconnected;

  /** @override @method @returns {void} */
  constructor({ connected }) {
    super(0.0, null, true);

    this.#connected = connected;

    const [icon_connected, icon_disconnected] = resource(
      "icons/scalable/actions/q-connected.svg",
      "icons/scalable/actions/q-disconnected.svg",
    );

    this.#icon_connected = Gio.Icon.new_for_string(icon_connected);
    this.#icon_disconnected = Gio.Icon.new_for_string(icon_disconnected);

    this.#icon = new St.Icon({
      gicon: this.#connected ? this.#icon_connected : this.#icon_disconnected,
      style_class: "system-status-icon",
      reactive: true,
      track_hover: true,
      visible: !connected,
    });

    this.add_child(this.#icon);

    this.#connection = this.connect("notify::connected", () => {
      if (this.#connected) {
        this.#icon.gicon = this.#icon_connected;
        this.#icon.visible = false;
      } else {
        this.#icon.gicon = this.#icon_disconnected;
        this.#icon.visible = true;
      }
    });
  }

  /** @override @method @returns {void} */
  vfunc_finalize() {
    this.disconnect(this.#connection);

    this.#connected = null;
    this.#icon = null;
    this.#icon_connected = null;
    this.#icon_disconnected = null;

    super.vfunc_finalize();
  }

  /** @override @method @param {import("../types/.clutter").Event} event @returns {boolean} */
  vfunc_event(event) {
    if (this.menu && event.type() === Clutter.EventType.BUTTON_PRESS)
      Gio.Subprocess.new(["q"], Gio.SubprocessFlags.NONE);

    return Clutter.EVENT_PROPAGATE;
  }
}

/**
 * A utility class used to manage the execution of promises.
 *
 * In the context of this extension, it is used to ensure that the extension
 * never enters an invalid state by only allowing execution of one promise at
 * a time, but skipping cancellable promises if there are more promises that
 * need to be started.
 *
 * All of these promises are wrapped using the `Queue.Item` class.
 *
 * @private
 * @class
 */
class Queue {
  /**
   * A unit of work that can be started by a `Queue`.
   *
   * @public
   * @static
   * @property
   */
  static Item = class Item {
    _: unknown;

    /**
     * Creates a new `Queue.Item`.
     *
     * If `entry` returns a cancellable promise-like object, then the queue will
     * cancel this item if there are other items waiting to be started.
     *
     * @public
     * @constructor
     * @template {any[]} A
     * @param {(...A) => PromiseLike<any>} entry A function to be called when
     * the item is started.
     * @param  {...A} args One or more values to be passed to `entry` when the
     * item is started.
     */
    constructor(entry, ...args) {
      this._ = () => entry(...args);
    }
  };

  /** @private @property @type {Queue.Item[]} */
  #items;
  /** @private @property @type {() => void} */
  #on_item_push;
  /** @private @property @type {boolean} */
  #running;

  /**
   * Creates a new empty queue without starting it.
   *
   * @public
   * @constructor
   */
  constructor() {
    this.#items = [];
    this.#on_item_push = () => {};
    this.#running = false;
  }

  /**
   * Pushes an `item` to the queue.
   *
   * If the queue **is not already running**, it will quietly start the queue
   * in the background and begin running its items.
   *
   * @public
   * @method
   * @param {Queue.Item} new_item The item to push to the queue.
   * @returns {this} for daisy chaining.
   * @throws {TypeError} if `item` is not not an instance of `Queue.Item`.
   */
  push(new_item) {
    if (!(new_item instanceof Queue.Item))
      throw TypeError("Expected a Queue.Item");

    this.#items.push(new_item);

    if (this.#running) {
      this.#on_item_push();
      return this;
    }

    (async () => {
      this.#running = true;

      let item = this.#items.shift();
      while (item) {
        try {
          const value = item._();

          if ("cancel" in value && "promise" in value) {
            const { promise, cancel } = value;

            if (this.#items.length === 0) {
              await promise;

              this.#on_item_push = () => {};
            } else {
              await cancel();
            }
          } else if (value instanceof Promise) {
            await value;
          }
        } catch (error) {
          log_msg(`Uncaught error in Queue: ${error}`);
        }

        item = this.#items.shift();
      }

      this.#running = false;
    })();

    return this;
  }
}

/**
 * Initializes the extension, without enabling it, and returns it. This function
 * is expected to be in the toplevel of every extension.
 *
 * @returns {Extension}
 */
// function init() {
//   debug("Initializing Amazon Q for CLI Extension");
//   var ExtensionClass = GObject.registerClass(
//     {
//       GTypeName: "AmazonQForCLIExtension",
//       Properties: {
//         connected: GObject.ParamSpec.boolean(
//           "connected",
//           "connected",
//           "connected",
//           GObject.ParamFlags.READWRITE,
//           false,
//         ),
//       },
//     },
//     QCliExtension,
//   );//

//   GObject.registerClass(
//     {
//       GTypeName: "AmazonQForCLIPanelIcon",
//       Properties: {
//         connected: GObject.ParamSpec.boolean(
//           "connected",
//           "connected",
//           "connected",
//           GObject.ParamFlags.READWRITE,
//           false,
//         ),
//       },
//     },
//     PanelIcon,
//   );//

//   return new ExtensionClass();
// }
