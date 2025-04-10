import { Intro, PrefSection } from "@/components/preference/list";
import { AUTOCOMPLETE_WIKI_URL, PRODUCT_NAME } from "@/lib/constants";

const autocompleteSettings: PrefSection[] = [
  {
    title: "General",
    properties: [
      {
        id: "autocomplete.disable",
        title: "Enable Autocomplete",
        description: `${PRODUCT_NAME} will provide a list of subcommands and options for you to choose from.`,
        type: "boolean",
        default: false,
        inverted: true,
        popular: true,
        priority: 1,
      },
      {
        id: "autocomplete.insertSpaceAutomatically",
        title:
          "Automatically insert a space after selecting a subcommand/option that takes a mandatory argument",
        description:
          "Autocomplete will insert a space after you select a suggestion that contains a mandatory argument",
        example: "e.g. selecting `clone` in `git clone`",
        type: "boolean",
        default: true,
        popular: true,
      },
      {
        id: "autocomplete.immediatelyExecuteAfterSpace",
        title: "Allow instant execute after Space",
        description:
          "Immediately execute commands after pressing [space]. This is helpful for users that habitually type a space before executing a command",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.scrollWrapAround",
        title: "Wrap around",
        description:
          "If true, when the end of suggestions are reached by pressing the down arrow key, it will wrap back around to the top.",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.alwaysSuggestCurrentToken",
        title: "Always suggest the current token",
        description:
          "Always add the current entered token as a suggestion at the top of the list",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.fuzzySearch",
        title: "Fuzzy search",
        description:
          "Search suggestions using substring matching rather than prefix search.",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.preferVerboseSuggestions",
        title: "Prefer verbose suggestions",
        description:
          "Use the verbose version of the option/subcommand inserted.",
        example:
          "e.g. selecting `-m, --message` suggestion will insert `--message` rather than `-m`.",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.onlyShowOnTab",
        title: "Suggest on [tab]",
        description:
          "Only show autocomplete when [tab] is pressed instead of showing it automatically.",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.disableForCommands",
        title: "Disable CLIs",
        description: `A comma separated list of CLI tools that ${PRODUCT_NAME} should not autocomplete on.`,
        example: "e.g. `git,npm,cd,docker`",
        type: "textlist",
        default: [],
        popular: false,
      },
      {
        id: "autocomplete.sortMethod",
        title: "Sort suggestions",
        description: `Specifies how ${PRODUCT_NAME} should sort suggestions.`,
        type: "select",
        default: "most recent",
        options: ["most recent", "alphabetical"],
        popular: false,
      },
      {
        id: "autocomplete.scriptTimeout",
        title: "Script timeout",
        description:
          "Specify the timeout in ms for scripts executed in completion spec generators.",
        type: "number",
        default: 5000,
        popular: false,
      },
      {
        id: "autocomplete.immediatelyRunDangerousCommands",
        title: "Allow instant execute of dangerous suggestions",
        description:
          'If true, users will be able to immediately run suggestions that completion specs have marked as "dangerous" rather than having to hit [enter] twice.',
        example: "e.g. `rm -rf`",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.immediatelyRunGitAliases",
        title: "Safe git aliases",
        description:
          "When disabled, Autocomplete will prevent instant execution of git aliases.",
        type: "boolean",
        default: true,
        popular: false,
      },
      {
        id: "autocomplete.firstTokenCompletion",
        title: "First token completion",
        description:
          "Offer completions for the CLI itself, not just its subcommand, options, and arguments.",
        example: "e.g. `cd`, `git`, etc.",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.hideAutoExecuteSuggestion",
        title: "Always hide auto-execute suggestions",
        description:
          "Hide suggestions that will be ran automatically on insert.",
        type: "boolean",
        default: false,
        popular: false,
      },
    ],
  },
  {
    title: "Appearance",
    // withPreview: true,
    properties: [
      {
        id: "autocomplete.theme",
        title: "Theme",
        type: "select",
        options: [
          "system",
          "dark",
          "light",
          "cobalt",
          "cobalt2",
          "dracula",
          "github-dark",
          "gruvbox",
          "monokai-dark",
          "nightowl",
          "nord",
          "panda",
          "poimandres",
          "the-unnamed",
          "synthwave-84",
          "solarized-light",
          "solarized-dark",
        ],
        default: "system",
        popular: true,
      },
      {
        id: "autocomplete.height",
        title: "Window height",
        type: "number",
        default: 140,
        popular: false,
      },
      {
        id: "autocomplete.width",
        title: "Window width",
        type: "number",
        default: 320,
        popular: false,
      },
      {
        id: "autocomplete.fontFamily",
        title: "Font family",
        default: null,
        type: "text",
        popular: false,
      },
      {
        id: "autocomplete.fontSize",
        title: "Font size",
        default: null,
        type: "number",
        popular: false,
      },
      // {
      //   id: "autocomplete.hidePreviewWindow",
      //   title: "Hide Preview window",
      //   description:
      //     "Hide the Preview window that appears on the side of the Autocomplete window.",
      //   type: "boolean",
      //   default: false,
      //   popular: false,
      // },
      // {
      //   id: "autocomplete.iconTheme",
      //   title: "Icon theme",
      //   description: "Change the theme where icons are pulled from.",
      //   type: "text"
      //   default: null,
      //   popular: false
      // },
    ],
  },
  {
    title: "Developer",
    properties: [
      {
        id: "autocomplete.developerMode",
        title: "Dev mode",
        description:
          "Turns off completion-spec caching and loads completion specs from the Specs Folder specified in the setting below.",
        example: "Developer Mode changes the way specs are loaded.",
        type: "boolean",
        default: false,
        popular: false,
      },
      {
        id: "autocomplete.devCompletionsFolder",
        title: "Specs folder",
        description: `When Developer Mode is enabled, ${PRODUCT_NAME} loads completion specs from the specified directory.`,
        type: "text",
        default: null,
        popular: false,
      },
    ],
  },
];

export const intro: Intro = {
  title: "CLI Completions",
  description: "IDE-style autocomplete for 500+ CLIs.",
  link: AUTOCOMPLETE_WIKI_URL,
};

export default autocompleteSettings;
