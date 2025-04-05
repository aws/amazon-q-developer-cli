# Amazon Q on Linux

## Installation

### Ubuntu/Debian

```bash
# Download the latest .deb package
curl -LO https://d3op2l77j7wnti.cloudfront.net/amazon-q/latest/amazon-q-latest-amd64.deb

# Install the package
sudo dpkg -i amazon-q-latest-amd64.deb
sudo apt-get install -f
```

### AppImage

```bash
# Download the latest AppImage
curl -LO https://d3op2l77j7wnti.cloudfront.net/amazon-q/latest/amazon-q-latest-x86_64.AppImage

# Run it directly (executable permissions are already set)
./amazon-q-latest-x86_64.AppImage
```

### Alternative Linux Builds

For other Linux distributions, you can download the appropriate package from:
- [Amazon Q Developer CLI Downloads](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html#command-line-installing-alternative-linux)

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
