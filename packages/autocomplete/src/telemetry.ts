import { Telemetry } from "@aws/amazon-q-developer-cli-api-bindings";
import { version } from "../package.json";

export const trackEvent = (
  event: string,
  props: Record<string, string | boolean | number | null>,
) =>
  Telemetry.track(event, {
    ...props,
    autocomplete_engine_version: version,
  });
