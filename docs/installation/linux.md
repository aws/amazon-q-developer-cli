# Amazon Q on Linux

## Installation

### Direct Download

#### Linux x86-64
```bash
curl --proto '=https' --tlsv1.2 -sSf "https://desktop-release.codewhisperer.us-east-1.amazonaws.com/latest/q-x86_64-linux.zip" -o "q.zip"
unzip q.zip
q/install.sh
```

#### Linux ARM (aarch64)
```bash
curl --proto '=https' --tlsv1.2 -sSf "https://desktop-release.codewhisperer.us-east-1.amazonaws.com/latest/q-aarch64-linux.zip" -o "q.zip"
unzip q.zip
q/install.sh
```

## Getting Started

After installation, simply run:

```bash
q login
```

> Most developers will log in using Builder ID as it is the simplest way to authenticate. Enterprise developers will likely authenticate using IAM Identity Center.

This will guide you through the authentication process and help you customize your installation. Once complete, open a new terminal session to start using Autocomplete and the `q` CLI.

## Support and Uninstall

If you're having issues with your installation, first run

```shell
q doctor
```

If that fails to resolve your issue, see our [support guide](../support.md). Otherwise run the following command to uninstall Amazon Q

```bash
q uninstall
```
