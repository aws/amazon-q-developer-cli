# Amazon Q on Linux

## Installation

### Direct Download

#### Linux x86-64
```bash
curl --proto '=https' --tlsv1.2 -sSf "https://desktop-release.codewhisperer.us-east-1.amazonaws.com/latest/q-x86_64-linux.zip" -o "q.zip"
unzip q.zip
sudo mv q /usr/local/bin/
```

#### Linux ARM (aarch64)
```bash
curl --proto '=https' --tlsv1.2 -sSf "https://desktop-release.codewhisperer.us-east-1.amazonaws.com/latest/q-aarch64-linux.zip" -o "q.zip"
unzip q.zip
sudo mv q /usr/local/bin/
```

## Getting Started

After installation, simply run:

```bash
q login
```

This will guide you through the authentication process (Builder ID or IAM Identity Center) and help you customize your installation. Once complete, open a new terminal session to start using Autocomplete and the `q` CLI.

## Support and Uninstall

If you're having issues with your installation, first run

```shell
q doctor
```

If that fails to resolve your issue, see our [support guide](../support.md). Otherwise run the following command to uninstall Amazon Q

```bash
q uninstall
```
