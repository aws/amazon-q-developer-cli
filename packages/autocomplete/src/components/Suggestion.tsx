import { useCallback, useMemo, CSSProperties, useRef } from "react";
import logger from "loglevel";
import fuzzysort from "@aws/amazon-q-developer-cli-fuzzysort";
import { Suggestion as SuggestionT } from "@aws/amazon-q-developer-cli-shared/internal";
import { makeArray } from "@aws/amazon-q-developer-cli-shared/utils";
import { getQueryTermForSuggestion } from "../suggestions/helpers";
import SuggestionIcon from "./SuggestionIcon";

type SuggestionProps = {
  style: CSSProperties;
  suggestion: SuggestionT;
  onClick: (item: SuggestionT) => void;
  isActive: boolean;
  commonPrefix: string;
  iconPath: string;
  searchTerm: string;
  iconSize: number;
  fuzzySearchEnabled: boolean;
};

type HighlightType = "match" | "prefix";

type Text =
  | {
      id: number;
      type: "text";
      text: string;
    }
  | {
      id: number;
      type: HighlightType;
      highlight: string;
    };

const getTitle = (
  item: SuggestionT,
  commonPrefix: string,
  searchTerm: string,
  fuzzySearch: boolean,
) => {
  let elementId = 0;

  const nameArray = item.displayName
    ? [item.displayName]
    : (makeArray(item.name) as string[]);

  if (nameArray.length === 0) {
    return <></>;
  }

  const queryTerm = getQueryTermForSuggestion(item, searchTerm);

  let out: Text[][] = [];

  if (queryTerm) {
    if (fuzzySearch) {
      // Recompute if there is a displayName, fuzzyMatchData comes from name array.
      const matches =
        item.displayName || !item.fuzzyMatchData
          ? nameArray.map((name) => fuzzysort.single(queryTerm, name))
          : item.fuzzyMatchData;

      out = matches.map((result, index) => {
        if (result) {
          let highlighted: Text[] = [];
          for (let i = 0; i < result.target.length; i += 1) {
            // if a char is in the indexes it is highlighted
            if (result.indexes.includes(i)) {
              highlighted.push({
                id: elementId++,
                type: "match",
                highlight: result.target[i],
              });
            } else {
              highlighted.push({
                id: elementId++,
                type: "text",
                text: result.target[i],
              });
            }
          }

          highlighted = highlighted.reduce((acc, curr) => {
            if (acc.length > 0) {
              const last = acc[acc.length - 1];
              if (curr.type === "text" && last.type === "text") {
                return [
                  ...acc.slice(0, -1),
                  {
                    id: elementId++,
                    type: "text",
                    text: last.text + curr.text,
                  },
                ];
              }
              if (
                curr.type !== "text" &&
                last.type !== "text" &&
                last.type === curr.type
              ) {
                return [
                  ...acc.slice(0, -1),
                  {
                    id: elementId++,
                    type: last.type,
                    highlight: last.highlight + curr.highlight,
                  },
                ];
              }
            }
            return [...acc, curr];
          }, [] as Text[]);

          return highlighted;
        }
        return [
          {
            id: elementId++,
            type: "text",
            text: nameArray[index],
          },
        ];
      });
    } else {
      out = nameArray.map((name) => {
        try {
          const matches = name?.match(new RegExp(`^(${queryTerm})(.*)`, "i"));
          if (matches) {
            return [
              {
                id: elementId++,
                type: "match",
                highlight: matches[1],
              },
              {
                id: elementId++,
                type: "text",
                text: matches[2],
              },
            ];
          }
          return [
            {
              id: elementId++,
              type: "text",
              text: name,
            },
          ];
        } catch (_err) {
          return [
            {
              id: elementId++,
              type: "text",
              text: name,
            },
          ];
        }
      });
    }
  } else {
    out = nameArray.map((name) => [
      {
        id: elementId++,
        type: "text",
        text: name,
      },
    ]);
  }

  if (commonPrefix && commonPrefix !== "-" && commonPrefix.length > 0) {
    out = out.map((name) => {
      try {
        if (name.length > 0 && name[0].type === "text") {
          const matches = name[0].text.match(
            new RegExp(`^(${commonPrefix})(.*)`, "i"),
          );
          if (matches) {
            return [
              {
                id: elementId++,
                type: "prefix",
                highlight: matches[1],
              },
              {
                id: elementId++,
                type: "text",
                text: matches[2],
              },
              ...name.slice(1),
            ];
          }
        } else if (
          name.length > 1 &&
          queryTerm.length > 0 &&
          name[0].type !== "text" &&
          name[1].type === "text"
        ) {
          const matches = name[1].text.match(
            new RegExp(`^(${commonPrefix.slice(queryTerm.length)})(.*)`, "i"),
          );
          if (matches) {
            return [
              name[0],
              {
                id: elementId++,
                type: "prefix",
                highlight: matches[1],
              },
              {
                id: elementId++,
                type: "text",
                text: matches[2],
              },
              ...name.slice(2),
            ];
          }
        }
        return name;
      } catch (err) {
        logger.warn("Error annotating marks for suggestion", err);
      }
      return name;
    });
  }

  const argString = makeArray(item.args || [])
    .filter((arg) => arg.name)
    .map((arg) => {
      const baseName = arg.isVariadic ? `${arg.name}...` : arg.name;
      return arg.isOptional ? `[${baseName}]` : `<${baseName}> `;
    })
    .join(" ");

  // Return output from adding highlighting and underlining + displaying args in brackets...
  return (
    <>
      {out.map((e, i) => (
        <>
          {i !== 0 ? <span key={elementId++}>{", "}</span> : null}
          {e.map((t) => {
            if (t.type === "text") {
              return <span key={t.id}>{t.text}</span>;
            }
            if (t.type === "match") {
              return (
                <mark
                  className="bg-matching-bg/80 group-data-[active-item]:bg-selected-matching-bg/80 text-inherit brightness-95"
                  key={t.id}
                >
                  <span className="brightness-125">{t.highlight}</span>
                </mark>
              );
            }
            if (t.type === "prefix") {
              return (
                <mark
                  className="bg-transparent text-inherit underline"
                  key={t.id}
                >
                  {t.highlight}
                </mark>
              );
            }
            return <span key={t.id}>{t.highlight}</span>;
          })}
        </>
      ))}
      <span className="opacity-50"> {argString} </span>
    </>
  );
};

const Suggestion = ({
  style,
  suggestion,
  commonPrefix,
  searchTerm,
  fuzzySearchEnabled,
  iconPath,
  isActive,
  onClick,
  iconSize,
}: SuggestionProps) => {
  const onSuggestionClick = useCallback(() => {
    onClick(suggestion);
  }, [onClick, suggestion]);

  const textContainerRef = useRef<HTMLDivElement>(null);
  const textRef = useRef<HTMLDivElement>(null);

  const Title = useMemo(
    () => getTitle(suggestion, commonPrefix, searchTerm, fuzzySearchEnabled),
    [suggestion, commonPrefix, searchTerm, fuzzySearchEnabled],
  );

  return (
    <div
      style={style}
      className={`flex items-center overflow-hidden pl-1.5 ${
        isActive ? "bg-selected-bg brightness-95" : ""
      }`}
      onClick={onSuggestionClick}
    >
      <SuggestionIcon
        style={{
          height: iconSize,
          display: "flex",
          width: iconSize,
          minWidth: iconSize,
          marginRight: 5,
        }}
        suggestion={suggestion}
        iconPath={iconPath}
      />
      <div className="overflow-hidden" ref={textContainerRef}>
        <div
          className="text data-[active-item]:text-selected-text group w-fit whitespace-nowrap"
          // style={{
          //   animation:
          //     isActive &&
          //     textOverflow &&
          //     suggestion.name &&
          //     !suggestion.args?.find((arg) => arg.name)
          //       ? // We only want to animate if there are no args that will be displayed in the title
          //         `${animationDuration}s linear infinite marquee`
          //       : undefined,
          // }}
          data-active-item={isActive ? true : undefined}
          ref={textRef}
        >
          {Title}
        </div>
      </div>
    </div>
  );
};

export default Suggestion;
