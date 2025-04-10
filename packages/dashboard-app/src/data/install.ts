import { InstallCheckWithInstallKey } from "@/types/preferences";
import accessibility_fig from "@assets/images/accessibility_fig.png?url";
import desktop_entry from "@assets/images/desktop_entry.png?url";
import gnome_shell_extension from "@assets/images/gnome_shell_extension.png?url";
import { DESKTOP_ENTRY_NAME, PRODUCT_NAME } from "@/lib/constants";
import { Platform } from "@aws/amazon-q-developer-cli-api-bindings";

const getBackupsDir = (): string => {
  const backupsDir = window?.fig?.constants?.backupsDir;
  const home = window?.fig?.constants?.home;
  if (backupsDir && home) {
    return backupsDir.replace(home, "~") + "/";
  } else {
    return "~/.amazon-q.dotfiles.bak/";
  }
};

export const gnomeExtensionInstallCheck = {
  id: "gnomeExtension",
  installKey: "gnomeExtension",
  platformRestrictions: {
    os: Platform.Os.LINUX,
    desktopEnvironment: Platform.DesktopEnvironment.GNOME,
    displayServerProtocol: Platform.DisplayServerProtocol.WAYLAND,
  },
  title: "Install the GNOME Shell Extension",
  description: [
    `In order for ${PRODUCT_NAME} to function properly under Wayland, we need to install the ${PRODUCT_NAME} GNOME Shell Extension.`,
    `New GNOME Shell extensions will require you to restart your current session by logging out.`,
  ],
  image: gnome_shell_extension,
  action: "Install",
};

const installChecks: InstallCheckWithInstallKey[] = [
  {
    id: "dotfiles",
    installKey: "dotfiles",
    title: "Shell integrations",
    description: [
      `Integrate ${PRODUCT_NAME} with your local shell so we can run the necessary hooks.`,
    ],
    action: "Install",
    explainer: {
      title: "What's happening under the hood?",
      steps: [
        [
          {
            content: `${PRODUCT_NAME} will add one line to the top and bottom of the following files (if they exist):`,
            tag: "span",
          },
          {
            content: ".zshrc",
            tag: "code",
          },
          {
            content: ".zprofile",
            tag: "code",
          },
          {
            content: ".bashrc",
            tag: "code",
          },
          {
            content: ".bash_profile",
            tag: "code",
          },
          {
            content: ".profile",
            tag: "code",
          },
        ],
        [
          {
            content: "Your dotfiles will be backed up to",
            tag: "span",
          },
          {
            content: getBackupsDir(),
            tag: "code",
          },
        ],
        [
          {
            content: `Uninstalling ${PRODUCT_NAME} will remove these lines.`,
            tag: "span",
          },
        ],
      ],
    },
  },
  {
    id: "accessibility",
    installKey: "accessibility",
    platformRestrictions: {
      os: Platform.Os.MACOS,
    },
    title: "Enable accessibility",
    description: [
      `Grant accessibility permissions so ${PRODUCT_NAME} can position the completion window and insert text on your behalf.`,
      `If enabling it isn't working, try toggling it off and on again or restarting ${PRODUCT_NAME}.`,
    ],
    image: accessibility_fig,
    action: "Enable",
    actionWaitingText: "Waiting for permission...",
  },
  {
    id: "gnomeExtension",
    installKey: "gnomeExtension",
    platformRestrictions: {
      os: Platform.Os.LINUX,
      desktopEnvironment: Platform.DesktopEnvironment.GNOME,
      displayServerProtocol: Platform.DisplayServerProtocol.WAYLAND,
    },
    title: "Install the GNOME Shell Extension",
    description: [
      `In order for ${PRODUCT_NAME} to function properly under Wayland, we need to install the ${PRODUCT_NAME} GNOME Shell Extension.`,
      `New GNOME Shell extensions will require you to restart your current session by logging out.`,
    ],
    image: gnome_shell_extension,
    action: "Install",
  },
  {
    id: "desktopEntry",
    installKey: "desktopEntry",
    platformRestrictions: {
      os: Platform.Os.LINUX,
      appBundleType: Platform.AppBundleType.APPIMAGE,
    },
    title: "Install Desktop Entry",
    description: [
      `The AppImage bundle includes a desktop entry file that we can install locally for you automatically.`,
    ],
    explainer: {
      title: "What's happening under the hood?",
      steps: [
        [
          {
            content: "A desktop entry file will be installed under",
            tag: "span",
          },
          {
            content: `~/.local/share/applications/${DESKTOP_ENTRY_NAME}`,
            tag: "code",
          },
        ],
        [
          {
            content: "An autostart entry file will be installed under",
            tag: "span",
          },
          {
            content: `~/.config/autostart/${DESKTOP_ENTRY_NAME}`,
            tag: "code",
          },
        ],
        [
          {
            content: `Uninstalling ${PRODUCT_NAME} will remove these files.`,
            tag: "span",
          },
        ],
      ],
    },
    image: desktop_entry,
    skippable: true,
    action: "Install",
  },
  // {
  //   id: "inputMethod",
  //   installKey: "inputMethod",
  //   title: "Input methods",
  //   description:
  //     [`Integrate ${PRODUCT_NAME} with your local shell so we can run the necessary hooks.`],
  //   image: '/asdf',
  //     action: "Enable",
  // },
];

export default installChecks;
