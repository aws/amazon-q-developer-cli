import { useEffect, useState } from "react";
import { Switch } from "../ui/switch";
import { Pref, PrefDefault } from "@/types/preferences";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../ui/select";
import { Input } from "../ui/input";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import Keystroke from "../ui/keystrokeInput";
import { interpolateSettingBoolean } from "@/lib/utils";
import { useSetting } from "@/hooks/store";
import { parseBackticksToCode } from "@/lib/strings";
import IntegrationCard from "../installs/integrations";

export function Setting({
  data,
  disabled,
}: {
  data: Pref;
  disabled?: boolean;
}) {
  const [setting, setSetting] = useSetting(data.id);
  const [inputValue, setInputValue] = useState<PrefDefault>(
    setting ?? data.default,
  );

  // see if this specific setting is set in config file, then synchronize the initial state
  useEffect(() => {
    if (setting == undefined) {
      setInputValue(data.default);
    } else {
      setInputValue(setting);
    }
  }, [setting, data.default]);

  const localValue =
    data.type === "boolean"
      ? interpolateSettingBoolean(inputValue as boolean, data.inverted)
      : inputValue;

  const multiSelectValue = inputValue as string[];

  function toggleSwitch() {
    setSetting(!inputValue);
  }

  function setSelection(value: string) {
    setSetting(value);
  }

  function toggleMultiSelect(option: string) {
    if (multiSelectValue.includes(option)) {
      const index = multiSelectValue.indexOf(option);
      multiSelectValue.splice(index, 1);
      const updatedArray = multiSelectValue;
      setSetting(updatedArray);
      return;
    }

    const updatedArray = [...multiSelectValue, option];
    setSetting(updatedArray);
  }

  if (data.id.split(".")[0] === "integrations") {
    return (
      <IntegrationCard
        toggleSwitch={toggleSwitch}
        integration={data}
        enabled={localValue as boolean}
      />
    );
  }

  return (
    <div className={`flex p-4 pl-0 gap-4`}>
      {data.type !== "keystrokes" && (
        <div className="flex-none w-12">
          {data.type === "boolean" && (
            <Switch
              onClick={toggleSwitch}
              checked={localValue as boolean}
              disabled={disabled}
            />
          )}
        </div>
      )}
      <div className="flex flex-col gap-1">
        <h3 className="font-medium leading-none">{data.title}</h3>
        {data.description && (
          <p className="font-light text-sm">
            {/* {parseBracketsToKbd(data.description, 'py-[1px] text-xs -top-[1px]')} */}
            {data.description}
          </p>
        )}
        {data.example && (
          <p className="font-light leading-tight text-sm">
            {typeof data.example === "string"
              ? parseBackticksToCode(data.example, "py-[1px]")
              : data.example}
          </p>
        )}
        {data.type !== "boolean" && (
          <div className="pt-1">
            {/* single value <select> menu */}
            {data.type === "select" && (
              <Select
                disabled={disabled}
                onValueChange={setSelection}
                value={localValue as string}
              >
                <SelectTrigger className="w-60">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent className="max-h-64 overflow-auto">
                  <SelectGroup>
                    {data.options?.map((o, i) => {
                      // console.log({ pref: data.id, localValue, inputValue: o, equal: localValue === o })
                      return (
                        <SelectItem value={o} key={i}>
                          {o}
                        </SelectItem>
                      );
                    })}
                  </SelectGroup>
                </SelectContent>
              </Select>
            )}
            {/* multi-value <select> menu */}
            {data.type === "multiselect" && (
              <div className="relative">
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button variant="outline">Select options</Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent className="w-60">
                    {data.options?.map((o, i) => {
                      const included = multiSelectValue.includes(o) as boolean;
                      return (
                        <DropdownMenuCheckboxItem
                          key={i}
                          checked={included}
                          onCheckedChange={() => toggleMultiSelect(o)}
                        >
                          {o}
                        </DropdownMenuCheckboxItem>
                      );
                    })}
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            )}
            {/* for number values, currently only used for ms, thus the 1000-unit step */}
            {data.type === "number" && (
              <Input
                disabled={disabled}
                type="number"
                min={0}
                step={1}
                placeholder={
                  typeof data.default === "string"
                    ? data.default
                    : data.default?.toString()
                }
                value={typeof localValue === "number" ? localValue : ""}
                onChange={(e) => {
                  const value = e.target.value;
                  setSetting(value === "" ? 0 : Number(value));
                }}
              />
            )}
            {data.type === "textlist" && (
              <Input
                disabled={disabled}
                type="text"
                placeholder={
                  typeof data.default === "string"
                    ? data.default
                    : data.default?.toString()
                }
                value={Array.isArray(setting) ? setting.join(",") : ""}
                onChange={(e) => {
                  // replace all whitespace
                  const value = e.target.value;
                  setSetting(
                    value === ""
                      ? undefined
                      : value.split(",").map((v) => v.trim()),
                  );
                }}
              />
            )}
            {/* generic text input */}
            {data.type === "text" && (
              <Input
                disabled={disabled}
                type="text"
                placeholder={
                  typeof data.default === "string"
                    ? data.default
                    : data.default?.toString()
                }
                value={typeof setting === "string" ? setting : ""}
                onChange={(e) => {
                  const value = e.target.value;
                  setSetting(value === "" ? undefined : value);
                }}
              />
            )}
            {/* multi-keystroke value input */}
            {data.type === "keystrokes" && (
              <Keystroke id={data.id} defaults={data.default as string[]} />
            )}
          </div>
        )}
      </div>
    </div>
  );
}
