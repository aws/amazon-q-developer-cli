import React from "react";
import {
  Suggestion,
  SuggestionType,
} from "@aws/amazon-q-developer-cli-shared/internal";
import { localProtocol } from "@aws/amazon-q-developer-cli-shared/utils";
import { icons } from "../fig/icons";

type SuggestionIconProps = {
  suggestion: Suggestion;
  iconPath: string;
  style: React.CSSProperties;
};

const transformIconUri = (icon: URL): URL => {
  let { host } = icon;

  if (icon.protocol !== "fig:") {
    return icon;
  }

  if (host === "" && fig.constants?.newUriFormat) {
    host = "path";
  }

  if (icon.hostname === "icon") {
    const type = icon.searchParams.get("type");
    if (type) {
      if (icons.includes(type)) {
        return new URL(
          `https://specs.q.us-east-1.amazonaws.com/icons/${type}.png`,
        );
      }
    }
  }

  if (window.fig.constants?.os === "windows") {
    return new URL(
      `https://fig.${host}${icon.pathname}${icon.search}${icon.hash}`,
    );
  }

  return new URL(`fig://${host}${icon.pathname}${icon.search}${icon.hash}`);
};

function renderIcon(icon: URL, height: string | number) {
  const isFigProtocol = icon.protocol === "fig:";

  const isTemplate = isFigProtocol ? icon.host.endsWith("template") : false;

  const color = isFigProtocol ? icon.searchParams.get("color") : undefined;
  const badge = isFigProtocol ? icon.searchParams.get("badge") : undefined;
  const type = isFigProtocol ? icon.searchParams.get("type") : undefined;

  return (
    <div
      role="img"
      aria-label={
        type
          ? `Icon for ${type}`
          : isTemplate
            ? "Template icon"
            : `Icon for ${icon.pathname}`
      }
      className="grid overflow-hidden bg-contain bg-no-repeat"
      style={{
        height,
        width: height,
        minWidth: height,
        minHeight: height,
        fontSize: typeof height === "number" ? height * 0.6 : height,
        backgroundImage: isTemplate
          ? `url(${transformIconUri(new URL(`fig://template?color=${color}`))})`
          : `url(${icon})`,
      }}
    >
      {badge &&
        (isTemplate ? (
          <span
            className="place-self-center text-center text-white"
            style={{
              fontSize: typeof height === "number" ? height * 0.5 : height,
            }}
          >
            {badge}
          </span>
        ) : (
          <span
            className="flex h-2.5 w-2.5 place-content-center place-self-end bg-contain bg-no-repeat text-[80%] text-white"
            style={{
              backgroundImage: `url(${transformIconUri(
                new URL(`fig://template?color=${color}`),
              )})`,
            }}
          >
            {badge}
          </span>
        ))}
    </div>
  );
}

const SuggestionIcon = ({
  suggestion,
  iconPath,
  style,
}: SuggestionIconProps) => {
  const { icon, name, type } = suggestion;
  let img;
  let { height } = style;

  // The icon is a Emoji or text if it is <4 length
  if (icon && icon.length < 4) {
    if (typeof height === "number") {
      height *= 0.8;
    }
    img = (
      <span
        style={{
          fontSize: height,
        }}
        className="relative right-[0.0625rem] pb-2.5"
      >
        {icon}
      </span>
    );
  }

  if (!img && icon && typeof icon === "string") {
    try {
      const iconUri = new URL(icon);
      img = renderIcon(transformIconUri(iconUri), height ?? 0);
    } catch (_err) {
      if (typeof height === "number") {
        height *= 0.8;
      }
      img = (
        <span
          style={{
            fontSize: height,
          }}
          className="relative right-[0.0625rem] pb-2.5"
        >
          {icon}
        </span>
      );
    }
  }

  if (!img) {
    const srcMap: Partial<Record<SuggestionType | "other", URL>> = {
      folder: new URL(localProtocol("path", `${iconPath}${name}`)),
      file: new URL(localProtocol("path", `${iconPath}${name}`)),
      subcommand: transformIconUri(new URL("fig://icon?type=command")),
      option: transformIconUri(new URL("fig://icon?type=option")),
      shortcut: new URL(localProtocol("template", "?color=3498db&badge=💡")),
      "auto-execute": transformIconUri(new URL("fig://icon?type=carrot")),
      arg: transformIconUri(new URL("fig://icon?type=box")),
      mixin: new URL(localProtocol("template", "?color=628dad&badge=➡️")),
    };

    const src =
      (type && srcMap[type] ? srcMap[type] : undefined) ??
      new URL(localProtocol("icon", "?type=box"));

    img = renderIcon(src, height ?? 0);
  }

  return <div style={style}>{img}</div>;
};

export default SuggestionIcon;
