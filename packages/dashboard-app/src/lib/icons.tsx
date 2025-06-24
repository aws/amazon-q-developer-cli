import * as Icon from "@/components/svg/icons";

export function getIconFromName(name: string, size?: number) {
  switch (name.toLowerCase()) {
    case "what's new?":
    default:
      return <Icon.Sparkle size={size} />;
    case "help & support":
      return <Icon.Help size={size} />;
    case "autocomplete":
    case "cli completions":
      return <Icon.Autocomplete size={size} />;
    case "inline":
    case "inline shell completions":
      return <Icon.InlineShell size={size} />;
    case "translate":
    case "translation":
      return <Icon.Translate size={size} />;
    case "chat":
      return <Icon.Chat size={size} />;
    case "account":
      return <Icon.User size={size} />;
    case "integrations":
      return <Icon.Apps size={size} />;
    case "keybindings":
      return <Icon.Keybindings size={size} />;
    case "preferences":
      return <Icon.Settings size={size} />;
    case "getting started":
      return <Icon.Onboarding size={size} />;
  }
}
