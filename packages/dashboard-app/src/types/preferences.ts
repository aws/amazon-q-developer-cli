import { Platform } from "@aws/amazon-q-developer-cli-api-bindings";

export type PlatformInfo = Awaited<ReturnType<typeof Platform.getPlatformInfo>>;

export type PlatformRestrictions = Partial<PlatformInfo>;

export type PrefDefault = unknown;

export type Pref = {
  id: string;
  title: string;
  description?: string;
  example?: React.ReactNode;
  type: string;
  inverted?: boolean;
  default: PrefDefault;
  popular?: boolean;
  options?: string[];
  icon?: string;
  background?: {
    color: string;
    image: string;
  };
  priority?: number;
};

export type Action = {
  id: string;
  title: string;
  description?: string;
  availability: string;
  type: string;
  default: string[];
  popular?: boolean;
};

export type RichText = {
  content: string;
  tag: string;
};

export type InstallKey =
  | "dotfiles"
  | "accessibility"
  | "inputMethod"
  | "desktopEntry"
  | "gnomeExtension";

export type InstallCheck = {
  id: string;
  installKey?: InstallKey;
  platformRestrictions?: PlatformRestrictions;
  title: string;
  description: string[];
  image?: string;
  action: string;
  actionWaitingText?: string;
  skippable?: boolean;
  explainer?: {
    title: string;
    steps: RichText[][];
  };
};

export type InstallCheckWithInstallKey = InstallCheck & {
  installKey: InstallKey;
};
