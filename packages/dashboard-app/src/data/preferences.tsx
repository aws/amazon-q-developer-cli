import { Link } from "@/components/ui/link";
import { PRODUCT_NAME, TELEMETRY_WIKI_URL } from "@/lib/constants";

const generalPreferences = [
  {
    title: "Startup",
    properties: [
      {
        id: "app.launchOnStartup",
        title: "Launch on Start",
        description: `Start ${PRODUCT_NAME} automatically whenever you restart your computer.`,
        type: "boolean",
        default: true,
        popular: false,
      },
      // {
      //   id: "app.preferredTerminal",
      //   title: "Preferred Terminal",
      //   description:
      //     `Choose your preferred terminal for ${PRODUCT_NAME} to launch commands in.`,
      //   type: "select",
      //   options: [
      //     "VS Code",
      //     "iTerm2",
      //     "Hyper",
      //     "Alacritty",
      //     "Kitty",
      //     "Terminal",
      //   ],
      //   default: "Terminal",
      //   popular: false,
      // },
      // {
      //   id: "app.disableAutolaunch",
      //   title: "Open in new shells",
      //   description: "Automatically launch when opening a new shell session",
      //   type: "boolean",
      //   default: true,
      //   inverted: true,
      //   popular: false,
      // },
      {
        id: "app.disableAutoupdates",
        title: "Automatic updates",
        description:
          "Asynchronously check for updates when launching a new shell session.",
        type: "boolean",
        default: false,
        inverted: true,
        popular: false,
      },
      {
        id: "app.hideMenubarIcon",
        title: "Display Menu Bar icon",
        description: `${PRODUCT_NAME} icon will appear in the Menu Bar while ${PRODUCT_NAME} is running.`,
        type: "boolean",
        default: false,
        inverted: true,
        popular: false,
      },
    ],
  },
  {
    title: "Advanced",
    properties: [
      {
        id: "app.beta",
        title: "Beta",
        description:
          "Opt into more frequent updates with all the newest features (and bugs).",
        type: "boolean",
        default: false,
        popular: false,
      },
      // {
      //   id: "cli.tips.disabled",
      //   title: "Terminal Tips",
      //   description: "Offers tips at the top of your terminal on start",
      //   type: "boolean",
      //   default: false,
      //   popular: false,
      // },
      {
        id: "telemetry.enabled",
        title: "Telemetry",
        description: `Enable ${PRODUCT_NAME} for command line to send usage data to AWS`,
        example: (
          <>
            Learn more at{" "}
            <Link href={TELEMETRY_WIKI_URL}>{TELEMETRY_WIKI_URL}</Link>
          </>
        ),
        type: "boolean",
        default: true,
        popular: false,
      },
      {
        id: "codeWhisperer.shareCodeWhispererContentWithAWS",
        title: `Share ${PRODUCT_NAME} content with AWS`,
        description: `When checked, your content processed by ${PRODUCT_NAME} may be used for service improvement (except for content processed by the Professional ${PRODUCT_NAME} service tier). Unchecking this box will cause AWS to delete any of your content used for that purpose. The information used to provide the ${PRODUCT_NAME} service to you will not be affected.`,
        example: (
          <>
            Learn more at{" "}
            <Link href={TELEMETRY_WIKI_URL}>{TELEMETRY_WIKI_URL}</Link>
          </>
        ),
        type: "boolean",
        default: true,
        popular: false,
      },
    ],
  },
];

export default generalPreferences;
