# Quirks of GNOME Shell Extensions

GNOME Shell Extensions are directly loaded into the same JavaScript Realm as the
Shell itself, so they have access to the same global variables as the Shell.

Most notably, the `imports` object, which is an odd `Proxy`-like object that has
properties that correspond to files and folders in the `js` directory of the
GNOME Shell source code. For example:

- `imports.ui.main` => `gnome-shell/js/ui/main.js`
- `imports.misc.extensionUtils` => `gnome-shell/js/misc/extensionUtils.js`

See the [gnome-shell js Gitlab repository for reference](https://gitlab.gnome.org/GNOME/gnome-shell/-/tree/44.9/js/ui?ref_type=tags).

GNOME Shell extensions also use this to import their own code. The difference is
that they have to call `imports.misc.extensionUtils.getCurrentExtension`
to get a reference to themselves and then access the `imports` property on the
returned reference, which acts like the global `imports` object, except that its
properties correspond to files and folders in the extensions folder.

## Prerequisites

```sh
sudo apt install libgirepository1.0-dev
```

## Testing Changes

See <https://gjs.guide/extensions/development/debugging.html#reloading-extensions>

# Quirk (bug) of Node

The build scripts for this repo spawn a ton of promises to do work in
parallel, and for whatever reason, this causes node to have fatal errors when
exceptions are raised in some promises. So, if you get a fatal exception, it's
probably this.
