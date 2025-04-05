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

## Complete onboarding steps

> Most developers will log in using Builder ID as it is the simplest way to authenticate. Enterpise developers will likely authenticate using IAM Identity Center.

1. Log in when prompted.
2. Complete the onboarding steps in order to customize your install.
3. Open a new terminal session to start using Autocomplete and the `q` CLI.

## Support and Uninstall

If you're having issues with your installation, first run

```shell
q doctor
```

If that fails to resolve your issue, see our [support guide](../support.md). Otherwise run the following command to uninstall Amazon Q

```bash
q uninstall
```
