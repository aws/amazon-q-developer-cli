import { useEffect, useCallback, useRef, useMemo } from "react";
import { Suggestion, Arg } from "@aws/amazon-q-developer-cli-shared/internal";
import { getMaxHeight, POPOUT_WIDTH } from "../window";
import { useAutocompleteStore } from "../state";
import { AutocompleteAction } from "../actions";

export type DescriptionPosition = "unknown" | "left" | "right";

type DescriptionProps = {
  currentArg: Arg | null;
  hasSuggestions: boolean;
  selectedItem: Suggestion;
  shouldShowHint: boolean;
  position: DescriptionPosition;
  height: number;
};

const Description = ({
  currentArg,
  hasSuggestions,
  selectedItem,
  shouldShowHint,
  position,
  height,
}: DescriptionProps) => {
  const descriptionRef = useRef<HTMLDivElement>(null);
  const { settings } = useAutocompleteStore();

  const hint = useMemo(() => {
    const keys = Object.entries(settings).reduce((acc, pair) => {
      const [key, value] = pair;

      switch (value) {
        case AutocompleteAction.TOGGLE_DESCRIPTION:
        case AutocompleteAction.SHOW_DESCRIPTION:
        case AutocompleteAction.HIDE_DESCRIPTION:
          acc.push(key);
          break;
        default:
          break;
      }

      return acc;
    }, [] as string[]);

    return (
      keys
        .map((key) =>
          key
            .replace("autocomplete.keybindings.", "")
            .split("+")
            .map((token) => {
              switch (token) {
                case "cmd":
                case "command":
                case "meta":
                  return "⌘";
                case "control":
                case "ctrl":
                  return "⌃";
                case "shift":
                  return "⇧";
                case "option":
                case "opt":
                  return "⌥";
                default:
                  return token;
              }
            })
            .join(""),
        )
        .shift() ?? "⌃k"
    );
  }, [settings]);

  const setDescriptionScroll = useCallback(
    (scroll: number) => {
      if (descriptionRef && descriptionRef.current) {
        descriptionRef.current.scrollLeft = scroll;
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [descriptionRef.current],
  );

  useEffect(() => {
    setDescriptionScroll(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentArg]);

  const itemType = selectedItem?.type || "";
  const itemDescription =
    selectedItem?.description ||
    (["file", "folder"].includes(itemType) ? itemType : "");

  let description = itemDescription.trim();
  let name = "";

  if (!description) {
    description = currentArg?.description?.trim() || "";
    name = currentArg?.name?.trim() || "";
  }

  const emptyDescription = (
    <span className="text-[#8c8c8c]">No description</span>
  );

  if (!hasSuggestions || position === "unknown") {
    const stackNameAndDescription = !hasSuggestions && name && description;
    return hasSuggestions || name ? (
      <div
        id="description"
        ref={descriptionRef}
        className={hasSuggestions ? "withSuggestions" : "px-2"}
        style={{ height: stackNameAndDescription ? height * 2 : height }}
      >
        <span className="scrollbar-none flex-shrink overflow-x-auto overflow-y-hidden">
          {name && <strong>{name}</strong>}
          {name && description && ": "}
          {stackNameAndDescription && <br />}
          {!hasSuggestions || name
            ? description
            : description || emptyDescription}
        </span>
        {hasSuggestions && shouldShowHint && hint && (
          <div
            className="descriptionHint rounded"
            style={{ fontSize: height * 0.5 }}
          >
            <span>{hint}</span>
          </div>
        )}
      </div>
    ) : null;
  }

  const maxHeight = getMaxHeight();

  return (
    <div
      id="description"
      className="popout"
      ref={descriptionRef}
      style={{
        maxHeight,
        width: POPOUT_WIDTH,
      }}
    >
      <div
        className="flex w-full flex-col gap-1 pb-1 pt-0.5"
        style={{
          maxHeight: maxHeight - 10,
        }}
      >
        <div className="flex-shrink overflow-y-auto pl-1.5 pr-1">
          {itemDescription.trim() ? (
            <span
              style={{
                maxWidth: POPOUT_WIDTH,
                maxHeight: maxHeight - 10,
              }}
              className="self-stretch"
            >
              {itemDescription.trim()}
            </span>
          ) : (
            emptyDescription
          )}
        </div>
        {shouldShowHint && hint && (
          <div className="descriptionHint bg-description-border mx-1 w-fit justify-center self-end rounded text-xs">
            <span>{hint}</span>
          </div>
        )}
      </div>
    </div>
  );
};

export default Description;
