# Proto

We use [protocol buffers](https://developers.google.com/protocol-buffers/) as a
message format for inter process communication.

This folder defines three main protocols:

1. `local.proto` - Protocol for communication from local processes like
   `figterm` and the `fig` CLI to the desktop app
2. `fig.proto` - Protocol for communication between client Fig.js apps like
   autocomplete and the desktop app
3. `figterm.proto` - Protocol for sending commands from the CLI to `figterm`
4. `remote.proto` - Protocol for sending between `figterm` and the desktop app,
   intended to be secure for remote machines

## Setup

For any client, you must install the protobuf compiler:

```shell
brew install protobuf
```

**Client Installations**

| Client     | Command        |
| ---------- | -------------- |
| typescript | `pnpm install` |
| rust       | N/A\*          |

\* The rust build process handles the installation of the proto toolchain.

## Installation/Usage

To compile protos, run:

```shell
./build-ts.sh
```

## Deprecating an Amazon Q API

1. Edit `fig.proto` and add the `[deprecated=true]` annotation to the relevant
   fields
2. Add an inline comment specifying the version when this was changed applies
   using the following format: `//deprecated: 1.2.3`

## Contributing

**Adding to protos**

Just edit the appropriate proto file.
