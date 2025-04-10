import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function interpolateSettingBoolean(
  value: boolean | string,
  inverted?: boolean,
) {
  if (inverted) {
    return !value;
  } else {
    return value;
  }
}
