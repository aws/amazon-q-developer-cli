# Amazon Q Desktop

This is the main Amazon Q Desktop written in Rust. It should be
ready to run and start developing with if you follow the
instructions in the [root README](../README.md).

## Developing

1. Follow the instructions under [the dashboard README](../../packages/dashboard-app/README.md) to run the development server.
1. Run `cargo run`.
1. Once the UI opens, right click anywhere to inspect element, go to the console tab, and set `window.location.href`
to the URL of the dashboard development server.
   - Alternatively, you can use the `DASHBOARD_URL` environment variable instead of manually setting `window.location.href`, e,g. `DASHBOARD_URL=http://localhost:3433 cargo run`.
