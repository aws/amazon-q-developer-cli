# Amazon Q on macOS

## Installation

1. [Download Amazon Q for macOS](https://desktop-release.codewhisperer.us-east-1.amazonaws.com/latest/Amazon%20Q.dmg)
2. Navigate to where you downloaded Amazon Q and double-click on it.
3. You will be prompted to move the application into your **Applications** folder.
4. (optional) Execute `codesign -v /Applications/Amazon\ Q.app` and ensure there is no output. This means the code signature is valid.
5. Open Amazon Q from the **Applications** folder by double clicking the icon.
6. (optional) Add Amazon Q to your Dock by right clicking the application icon and choosing `Options/Keep in Dock`

## Complete onboarding steps

> Most developers will log in using Builder ID as it is the simplest way to authenticate. Enterpise developers will likely authenticate using IAM Identity Center.

1. Log in when prompted.
2. Complete the onboarding steps in order to customize your install.
3. Open a new terminal session to start using Autocomplete and the `qqq` CLI.

## Support and Uninstall

If you're having issues with your installation, first run

```shell
qqq doctor
```

If that fails to resolve your issue, see our [support guide](../support.md). Otherwise run the following command to uninstall Amazon Q

```bash
qqq uninstall
```
