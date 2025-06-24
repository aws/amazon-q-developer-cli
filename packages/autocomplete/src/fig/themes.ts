import logger from "loglevel";
import { fread } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { SystemTheme } from "../hooks/helpers";
import { MissingThemeError } from "./errors";

const THEMES = ["dark", "light"] as const;
type BuiltinThemeName = (typeof THEMES)[number];

export const builtInThemes: Record<BuiltinThemeName, ThemeDefinition> = {
  dark: {
    version: "1.0",
    theme: {
      backgroundColor: "rgb(48,48,48)",
      textColor: "rgb(180,180,180)",
      matchBackgroundColor: "rgb(95,89,56)",
      selection: {
        textColor: "rgb(253,253,253)",
        backgroundColor: "rgb(30,90,199)",
        matchBackgroundColor: "rgb(106,142,218)",
      },
      description: {
        textColor: "rgb(180,180,180)",
        borderColor: "rgb(65,65,65)",
      },
    },
  },
  light: {
    version: "1.0",
    theme: {
      backgroundColor: "rgb(254,254,254)",
      textColor: "rgb(7,7,7)",
      matchBackgroundColor: "rgb(255,239,152)",
      selection: {
        textColor: "rgb(253,253,253)",
        backgroundColor: "rgb(41,105,218)",
        matchBackgroundColor: "rgb(106,142,218)",
      },
      description: {
        textColor: "rgb(7,7,7)",
        borderColor: "rgb(199,199,199)",
      },
    },
  },
};

const shadeFiller = {
  dark: {
    shade0: "#181818",
    shade1: "#282828",
    shade2: "#383838",
    shade3: "#585858",
    shade4: "#b8b8b8",
    shade5: "#d8d8d8",
    shade6: "#e8e8e8",
    shade7: "#f8f8f8",
    accent0: "#ab4642",
    accent1: "#dc9656",
    accent2: "#f7ca88",
    accent3: "#a1b56c",
    accent4: "#86c1b9",
    accent5: "#7cafc2",
    accent6: "#ba8baf",
    accent7: "#a16946",
  },
  light: {
    shade0: "#f8f8f8",
    shade1: "#e8e8e8",
    shade2: "#d8d8d8",
    shade3: "#b8b8b8",
    shade4: "#585858",
    shade5: "#383838",
    shade6: "#282828",
    shade7: "#181818",
    accent0: "#ab4642",
    accent1: "#dc9656",
    accent2: "#f7ca88",
    accent3: "#a1b56c",
    accent4: "#86c1b9",
    accent5: "#7cafc2",
    accent6: "#ba8baf",
    accent7: "#a16946",
  },
};

interface ThemeDefinition {
  author?: {
    name: string;
    twitter?: string;
    github?: string;
  };
  version: string;

  theme?: {
    textColor: string;
    backgroundColor: string;
    matchBackgroundColor: string;
    selection: {
      textColor: string;
      backgroundColor: string;
      matchBackgroundColor?: string;
    };
    description: {
      borderColor: string;
      textColor: string;
    };
  };

  shade0?: string;
  shade1?: string;
  shade2?: string;
  shade3?: string;
  shade4?: string;
  shade5?: string;
  shade6?: string;
  shade7?: string;
  accent0?: string;
  accent1?: string;
  accent2?: string;
  accent3?: string;
  accent4?: string;
  accent5?: string;
  accent6?: string;
  accent7?: string;
}

export function isBuiltInTheme(theme: string): theme is BuiltinThemeName {
  return (THEMES as ReadonlyArray<string>).includes(theme);
}

const RGB_REGEX = /rgb\((\d+),(\d+),(\d+)\)/;

// We map hex and `rgb()` colors to rgb numbers with a space between each number
export const mapColor = (color: string) => {
  if (color.startsWith("#")) {
    const hex = color.slice(1);
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);

    // a is between 0 and 1
    let a: number;
    if (hex.length === 8) {
      a = parseInt(hex.slice(6, 8), 16) / 255;
    } else {
      a = 1;
    }

    return `${r * a} ${g * a} ${b * a}`;
  }

  const match = color.match(RGB_REGEX);
  if (match) {
    return `${match[1]} ${match[2]} ${match[3]}`;
  }

  console.warn(`Invalid color: ${color}`);

  return color;
};

function setCSSProperties(
  { version, theme, ...rest }: ThemeDefinition,
  systemTheme: SystemTheme,
) {
  const root = document.documentElement;

  let mainBgColor;
  let mainTextColor;
  let matchingBgColor;
  let descriptionTextColor;
  let descriptionBorderColor;
  let selectedBgColor;
  let selectedTextColor;
  let selectedMatchingBgColor;

  let shade0;
  let shade1;
  let shade2;
  let shade3;
  let shade4;
  let shade5;
  let shade6;
  let shade7;
  let accent0;
  let accent1;
  let accent2;
  let accent3;
  let accent4;
  let accent5;
  let accent6;
  let accent7;

  if (version === "1.0" && theme) {
    mainBgColor = mapColor(theme.backgroundColor);
    mainTextColor = mapColor(theme.textColor);
    matchingBgColor = mapColor(theme.matchBackgroundColor);
    descriptionTextColor = mapColor(theme.description.textColor);
    descriptionBorderColor = mapColor(theme.description.borderColor);
    selectedBgColor = mapColor(theme.selection.backgroundColor);
    selectedTextColor = mapColor(theme.selection.textColor);
    selectedMatchingBgColor = mapColor(
      theme.selection.matchBackgroundColor ?? "rgb(106, 142, 218)",
    );

    shade0 = mainBgColor;
    shade1 = descriptionBorderColor;
    shade2 = mapColor(rest?.shade2 ?? shadeFiller[systemTheme].shade2);
    shade3 = mapColor(rest?.shade3 ?? shadeFiller[systemTheme].shade3);
    shade4 = mapColor(rest?.shade4 ?? shadeFiller[systemTheme].shade4);
    shade5 = descriptionTextColor;
    shade6 = mainTextColor;
    shade7 = selectedTextColor;
    accent0 = mapColor(rest?.accent0 ?? shadeFiller[systemTheme].accent0);
    accent1 = mapColor(rest?.accent1 ?? shadeFiller[systemTheme].accent1);
    accent2 = mapColor(rest?.accent2 ?? shadeFiller[systemTheme].accent2);
    accent3 = mapColor(rest?.accent3 ?? shadeFiller[systemTheme].accent3);
    accent4 = mapColor(rest?.accent4 ?? shadeFiller[systemTheme].accent4);
    accent5 = mapColor(rest?.accent5 ?? shadeFiller[systemTheme].accent5);
    accent6 = mapColor(rest?.accent6 ?? shadeFiller[systemTheme].accent6);
    accent7 = mapColor(rest?.accent7 ?? shadeFiller[systemTheme].accent7);
  } else {
    shade0 = mapColor(rest?.shade0 ?? shadeFiller[systemTheme].shade0);
    shade1 = mapColor(rest?.shade1 ?? shadeFiller[systemTheme].shade1);
    shade2 = mapColor(rest?.shade2 ?? shadeFiller[systemTheme].shade2);
    shade3 = mapColor(rest?.shade3 ?? shadeFiller[systemTheme].shade3);
    shade4 = mapColor(rest?.shade4 ?? shadeFiller[systemTheme].shade4);
    shade5 = mapColor(rest?.shade5 ?? shadeFiller[systemTheme].shade5);
    shade6 = mapColor(rest?.shade6 ?? shadeFiller[systemTheme].shade6);
    shade7 = mapColor(rest?.shade7 ?? shadeFiller[systemTheme].shade7);
    accent0 = mapColor(rest?.accent0 ?? shadeFiller[systemTheme].accent0);
    accent1 = mapColor(rest?.accent1 ?? shadeFiller[systemTheme].accent1);
    accent2 = mapColor(rest?.accent2 ?? shadeFiller[systemTheme].accent2);
    accent3 = mapColor(rest?.accent3 ?? shadeFiller[systemTheme].accent3);
    accent4 = mapColor(rest?.accent4 ?? shadeFiller[systemTheme].accent4);
    accent5 = mapColor(rest?.accent5 ?? shadeFiller[systemTheme].accent5);
    accent6 = mapColor(rest?.accent6 ?? shadeFiller[systemTheme].accent6);
    accent7 = mapColor(rest?.accent7 ?? shadeFiller[systemTheme].accent7);

    // "polyfill" the rest of the colors
    mainBgColor = shade0;
    matchingBgColor = shade1;
    descriptionBorderColor = shade1;
    selectedBgColor = shade2;
    descriptionTextColor = shade5;
    mainTextColor = shade6;
    selectedTextColor = shade7;

    selectedMatchingBgColor = accent6;
  }

  root.style.setProperty("--main-bg-color", mainBgColor);
  root.style.setProperty("--main-text-color", mainTextColor);
  root.style.setProperty("--matching-bg-color", matchingBgColor);
  root.style.setProperty("--description-text-color", descriptionTextColor);
  root.style.setProperty("--description-border-color", descriptionBorderColor);
  root.style.setProperty("--selected-bg-color", selectedBgColor);
  root.style.setProperty("--selected-text-color", selectedTextColor);
  root.style.setProperty(
    "--selected-matching-bg-color",
    selectedMatchingBgColor,
  );

  // Set the rest of the colors
  root.style.setProperty("--shade0-color", shade0);
  root.style.setProperty("--shade1-color", shade1);
  root.style.setProperty("--shade2-color", shade2);
  root.style.setProperty("--shade3-color", shade3);
  root.style.setProperty("--shade4-color", shade4);
  root.style.setProperty("--shade5-color", shade5);
  root.style.setProperty("--shade6-color", shade6);
  root.style.setProperty("--shade7-color", shade7);
  root.style.setProperty("--accent0-color", accent0);
  root.style.setProperty("--accent1-color", accent1);
  root.style.setProperty("--accent2-color", accent2);
  root.style.setProperty("--accent3-color", accent3);
  root.style.setProperty("--accent4-color", accent4);
  root.style.setProperty("--accent5-color", accent5);
  root.style.setProperty("--accent6-color", accent6);
  root.style.setProperty("--accent7-color", accent7);
}

export async function setTheme(
  currentSystemTheme: SystemTheme,
  newTheme?: string,
) {
  try {
    if (!newTheme) throw new MissingThemeError();
    if (newTheme === "system") {
      setCSSProperties(builtInThemes[currentSystemTheme], currentSystemTheme);
      return;
    }
    if (isBuiltInTheme(newTheme)) {
      setCSSProperties(builtInThemes[newTheme], newTheme);
      return;
    }
    const theme: string | undefined = fig.constants?.themesFolder
      ? await fread(`${fig.constants.themesFolder}/${newTheme}.json`)
      : undefined;

    if (!theme) {
      throw new MissingThemeError(
        "Cannot find a local theme with the specified name",
      );
    }

    const parsedTheme = JSON.parse(theme) as ThemeDefinition;

    // All themes fallback to the dark theme if values are missing
    setCSSProperties({ ...builtInThemes.dark, ...parsedTheme }, "dark");
  } catch (err) {
    logger.info(
      "There was an error parsing the theme. Using default dark theme",
      err,
    );
    setCSSProperties(builtInThemes.dark, "dark");
  }
}

export function setFontFamily(fontFamily: string) {
  if (!fontFamily) return;
  const root = document.documentElement;
  root.style.setProperty("--font-family", fontFamily);
}

export function setFontSize(fontSize: number | undefined) {
  const root = document.documentElement;
  root.style.setProperty(
    "--font-size",
    !fontSize || fontSize <= 0 ? "12.8px" : `${fontSize}px`,
  );
}
