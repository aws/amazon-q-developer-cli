import { Check, Plus, X } from "lucide-react";
import { useCallback, useContext, useEffect, useState } from "react";
import {
  VALID_CONTROL_KEYS,
  getKeyName,
  getKeySymbol,
} from "@/lib/keybindings";
import ListenerContext from "@/context/input";
import { useKeybindings } from "@/hooks/store/useKeybindings";
import { useSetting } from "@/hooks/store";
import { cn } from "@/lib/utils";

export function Kbd({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <kbd
      className={cn(
        "p-1 py-[2px] not-italic text-black dark:text-white border border-black dark:border-white rounded-sm shadow-[0_4px_0_black] dark:shadow-[0_4px_0_white] relative -top-[2px]",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

export function Keystroke({ keybinding }: { keybinding: string }) {
  const setSetting = useSetting(`autocomplete.keybindings.${keybinding}`)[1];

  return (
    <button
      onClick={() => setSetting("ignore")}
      className="text-white/50 italic text-center text-xs flex justify-center gap-[2px] py-1 pl-[2px] group hover:bg-black dark:hover-bg-dusk-800 hover:text-white rounded-md p-1 px-2 items-center pr-0 hover:pr-2 transition-all"
    >
      {keybinding
        ? keybinding.split("+").map((l, i) => (
            <Kbd
              key={i}
              className="group-hover:text-white group-hover:border-white group-hover:shadow-[0_4px_0_white]"
            >
              {getKeySymbol(l)}
            </Kbd>
          ))
        : "press keys"}
      <X className="h-3 group-hover:w-3 w-0 ml-1 opacity-0 group-hover:opacity-100 -translate-x-full group-hover:translate-x-0 hover:bg-black/5 transition-transform" />
    </button>
  );
}

export function Input({
  command,
  value,
  invalid,
  cancel,
}: {
  command: string;
  value: string[];
  invalid: boolean;
  cancel: () => void;
}) {
  const joinedValue = value ? value.join("+") : null;
  const setSetting = useSetting(`autocomplete.keybindings.${joinedValue}`)[1];

  function handleNewKeystroke() {
    if (!value) {
      cancel();
      return;
    }

    if (invalid) return;

    setSetting(command);
    cancel();
  }

  return (
    <div className="flex gap-1">
      <button
        onClick={cancel}
        className="p-1 px-2 text-black hover:text-white hover:bg-red-500 rounded-sm"
      >
        <X className="w-3 h-3 dark:text-white" />
      </button>
      <div
        className={`flex items-stretch gap-1 p-[2px] py-1 ${
          !value && "pl-3"
        } text-xs bg-black rounded-md`}
      >
        <div className="text-white/50 italic text-center flex justify-center items-center gap-[2px] rounded-sm">
          {value
            ? value.map((k, i) => (
                <Kbd
                  key={i}
                  className="p-1 py-[2px] not-italic text-white border border-white rounded-sm shadow-[0_4px_0_white] relative -top-[2px]"
                >
                  {k}
                </Kbd>
              ))
            : "press keys"}
        </div>
        <button
          onClick={handleNewKeystroke}
          className="p-1 px-[6px] mx-[2px] text-white hover:bg-emerald-500 rounded-sm"
        >
          <Check className="w-3 h-3" />
        </button>
      </div>
    </div>
  );
}

export default function KeystrokeGroup({
  id,
  defaults,
}: {
  id: string;
  defaults: string[];
}) {
  // `id` has the `autocomplete.${command}` format. We just want the command.
  const command = id.split(".")[1];

  const keybindings = useKeybindings(command, defaults);
  const { listening, setListening } = useContext(ListenerContext);
  const [inputValue, setInputValue] = useState<string[] | null>(null);
  const [isInvalid, setIsInvalid] = useState(false);

  const inputOpen = listening === id;

  type keypressEvent = {
    key: string;
    keyCode: number;
    metaKey: boolean;
    ctrlKey: boolean;
    shiftKey: boolean;
    altKey: boolean;
    preventDefault: () => void;
    stopPropagation: () => void;
  };

  const handleKeyPress = useCallback((e: keypressEvent) => {
    const keys = new Set<string>();
    if (e.metaKey) keys.add("command");
    if (e.ctrlKey) keys.add("control");
    if (e.shiftKey) keys.add("shift");
    if (e.altKey) keys.add("option");
    const key = getKeyName(e.keyCode);

    const isInvalidCombination =
      keys.has("command") ||
      (keys.has("control") &&
        key !== "control" &&
        !VALID_CONTROL_KEYS.includes(key));
    setIsInvalid(isInvalidCombination);

    if (key) keys.add(key);
    setInputValue(Array.from(keys));
    e.preventDefault();
    e.stopPropagation();
  }, []);

  useEffect(() => {
    if (inputOpen) return;

    setIsInvalid(false);
    setInputValue(null);
  }, [inputOpen]);

  useEffect(() => {
    if (!inputOpen) return;
    // attach the event listener
    document.addEventListener("keydown", handleKeyPress);

    // remove the event listener
    return () => {
      document.removeEventListener("keydown", handleKeyPress);
    };
  }, [handleKeyPress, inputOpen]);

  function cancelKeystroke() {
    setInputValue(null);
    setListening(null);
  }

  function openInput() {
    setListening(id);
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex gap-2 flex-wrap">
        {keybindings.map((k: string, i: number) => (
          <Keystroke keybinding={k} key={i} />
        ))}
        {inputOpen ? (
          <Input
            command={command}
            value={inputValue as string[]}
            invalid={isInvalid}
            cancel={cancelKeystroke}
          />
        ) : (
          <button
            onClick={openInput}
            className="p-1 px-[6px] hover:bg-black/5 rounded-lg"
          >
            <Plus className="h-3 w-3" />
          </button>
        )}
      </div>
      {isInvalid && (
        <span className="text-xs font-medium text-red-500 pl-8">
          Sorry, that combination is invalid.
        </span>
      )}
    </div>
  );
}
