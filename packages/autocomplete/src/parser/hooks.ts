import React, { useEffect, useRef } from "react";
import { shallow } from "zustand/shallow";
import { parseArguments } from "@aws/amazon-q-developer-cli-autocomplete-parser";
import { useAutocompleteStore } from "../state";
import { shellContextSelector } from "../state/generators";

function usePrevious<T>(value: T) {
  const ref = useRef<T>(value);

  useEffect(() => {
    ref.current = value;
  }, [value]);

  return ref.current;
}

const isBufferDifferenceFromTyping = (
  oldBuffer: string,
  newBuffer: string,
): boolean => {
  // Determine whether the difference between two better states is likely from typing,
  // as opposed to pasting text or scrolling through history.
  if (!oldBuffer.startsWith(newBuffer) && !newBuffer.startsWith(oldBuffer)) {
    return false;
  }
  // TODO: maybe play with this threshold? For now we will allow only a difference
  // of one character to be considered typing.
  return Math.abs(oldBuffer.length - newBuffer.length) < 2;
};

export const useParseArgumentsEffect = (
  setLoading: React.Dispatch<React.SetStateAction<boolean>>,
) => {
  const setParserResult = useAutocompleteStore(
    (state) => state.setParserResult,
  );
  const command = useAutocompleteStore((state) => state.command);
  const onError = useAutocompleteStore((state) => state.error);
  const setVisibleState = useAutocompleteStore(
    (state) => state.setVisibleState,
  );
  const context = useAutocompleteStore(shellContextSelector, shallow);

  const oldCommand = usePrevious(command);

  useEffect(() => {
    let isMostRecentEffect = true;

    const tokens = command?.tokens || [];
    const oldTokens = oldCommand?.tokens || [];

    setLoading(true);
    // Only run if we didn't error in bash parser.
    parseArguments(command, context)
      .then((result) => {
        if (!isMostRecentEffect) return;
        setLoading(false);

        const hasBackspacedToNewToken =
          tokens.length < oldTokens.length &&
          oldTokens[tokens.length - 1].text === tokens[tokens.length - 1].text;

        const text = command?.originalTree.text ?? "";
        const oldText = oldCommand?.originalTree.text ?? "";
        const largeBufferChange = !isBufferDifferenceFromTyping(text, oldText);

        setParserResult(result, hasBackspacedToNewToken, largeBufferChange);
      })
      .catch((err) => {
        if (!isMostRecentEffect) return;
        setLoading(false);
        onError(err);
      });

    return () => {
      isMostRecentEffect = false;
    };
  }, [command, setParserResult, onError, context, setVisibleState]);
};
