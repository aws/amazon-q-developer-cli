import { PlatformInfo, PlatformRestrictions } from "@/types/preferences";

/**
 * @returns `true` if `restrictions` is undefined, or all fields in `restrictions` are equal to their corresponding entries in `platformInfo`. `false` otherwise.
 */
export function matchesPlatformRestrictions(
  platformInfo: PlatformInfo,
  restrictions?: PlatformRestrictions,
) {
  if (!restrictions) {
    return true;
  }

  return Object.entries(restrictions)
    .map(
      ([key, value]) =>
        key in platformInfo &&
        value === platformInfo[key as keyof PlatformInfo],
    )
    .reduce((prev, curr) => prev && curr, true);
}
