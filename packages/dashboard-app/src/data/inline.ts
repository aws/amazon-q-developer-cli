import { Intro, PrefSection } from "@/components/preference/list";
import { INLINE_WIKI_URL } from "@/lib/constants";

const inlineShellCompletionSettings: PrefSection[] = [
  {
    title: "Settings",
    properties: [
      {
        id: "inline.enabled",
        title: "Enable Inline completion",
        description: "This setting only applies in new shell sessions.",
        type: "boolean",
        default: true,
      },
    ],
  },
];

export const intro: Intro = {
  title: "Inline",
  description: "AI-generated command suggestions in your shell.",
  link: INLINE_WIKI_URL,
};

export default inlineShellCompletionSettings;
