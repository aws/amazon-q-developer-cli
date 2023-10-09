import { InstallCheck } from "@/types/preferences";

const installChecks: InstallCheck[] = [
  {
    id: "shellIntegrations",
    installKey: "dotfiles",
    title: "Shell integrations",
    description: [
      "Integrate CodeWhisperer with your local shell so we can run the necessary hooks.",
    ],
    action: "Install",
    explainer: {
      title: "What's happening under the hood?",
      steps: [
        [
          {
            content:
              "Fig will add one line to the top and bottom of the following files (if they exist):",
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
            content: "~/.codewhisperer.dotfiles.bak/",
            tag: "code",
          },
        ],
        [
          {
            content: "Uninstalling CodeWhisperer will remove these lines.",
            tag: "span",
          },
        ],
        [
          {
            content:
              "We've tested this installation thoroughly with over 100k users... It works just fine with even the most complicated dotfiles set ups!",
            tag: "span",
          },
        ],
      ],
    },
  },
  {
    id: "accessibility",
    installKey: "accessibility",
    title: "Accessibility settings",
    description: [
      "Fig uses this permission to position the Autocomplete window and insert text on your behalf.",
      "If enabling it isn't working, try toggling it off and on again or restarting Fig.",
    ],
    image: "/asdf",
    action: "Enable",
  },
  // {
  //   id: "inputMethod",
  //   installKey: "inputMethod",
  //   title: "Input methods",
  //   description:
  //     ["Integrate CodeWhisperer with your local shell so we can run the necessary hooks."],
  //   image: '/asdf',
  //     action: "Enable",
  // },
  {
    id: "login",
    title: "Signed in with Builder ID",
    description: [
      "AI features won't work if you're no longer signed into Builder ID.",
    ],
    action: "Sign in",
  },
];

export default installChecks;