import { Intro, PrefSection } from "@/components/preference/list";
import { CLI_BINARY_NAME, TRANSLATE_WIKI_URL } from "@/lib/constants";

const translateSettings: PrefSection[] = [
  {
    title: "Settings",
    properties: [
      {
        id: "ai.terminal-hash-sub",
        title: "Hashtag Substitution",
        example: `Comments in the shell will be substituted with the \`${CLI_BINARY_NAME} translate\` command.`,
        type: "boolean",
        default: true,
      },
      // {
      //   id: "ai.menu-actions",
      //   title: "Menu Actions",
      //   description: "The actions that will be available in the AI menu.",
      //   type: "multiselect",
      //   options: ["execute", "edit", "copy", "regenerate", "ask", "cancel"],
      //   default: ["execute", "edit", "regenerate", "ask", "cancel"],
      //   inverted: true,
      // },
    ],
  },
];

export const intro: Intro = {
  title: "Translate",
  description: `Translate natural language-to-bash. Just run \`${CLI_BINARY_NAME} translate\`.`,
  link: TRANSLATE_WIKI_URL,
};

export default translateSettings;
