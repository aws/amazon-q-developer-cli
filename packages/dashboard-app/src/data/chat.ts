import { Intro, PrefSection } from "@/components/preference/list";
import { CHAT_WIKI_URL, CLI_BINARY_NAME } from "@/lib/constants";

const chatSettings: PrefSection[] = [
  {
    title: "Settings",
    properties: [
      {
        id: "chat.greeting.enabled",
        title: "Enable chat greeting",
        description: "Enable the chat greeting message from chat starts.",
        type: "boolean",
        default: true,
      },
    ],
  },
];

export const intro: Intro = {
  title: "Chat",
  description: `Agentic AI in your command line. Just run \`${CLI_BINARY_NAME} chat\`.`,
  link: CHAT_WIKI_URL,
};

export default chatSettings;
