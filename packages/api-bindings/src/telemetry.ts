import { TelemetryProperty } from "@aws/amazon-q-developer-cli-proto/fig";
import {
  sendTelemetryIdentifyRequest,
  sendTelemetryPageRequest,
  sendTelemetryTrackRequest,
} from "./requests.js";

type Property = string | boolean | number | null;
type NamespaceSpecifier =
  | { username: string; id?: number }
  | { id: number; username?: string };

export function track(
  event: string,
  properties: Record<string, Property>,
  namespace?: NamespaceSpecifier,
) {
  // convert to internal type 'TelemetryProperty'
  const props = Object.keys(properties).reduce(
    (array, key) => {
      const entry: TelemetryProperty = {
        key,
        value: JSON.stringify(properties[key]),
      };
      array.push(entry);
      return array;
    },
    [] as unknown as [TelemetryProperty],
  );

  return sendTelemetryTrackRequest({
    event,
    properties: props,
    jsonBlob: JSON.stringify(properties),
    namespace: namespace?.username,
    namespaceId: namespace?.id,
  });
}

export function identify(traits: Record<string, Property>) {
  return sendTelemetryIdentifyRequest({ jsonBlob: JSON.stringify(traits) });
}

export function page(
  category: string,
  name: string,
  properties: Record<string, Property>,
) {
  // See more: https://segment.com/docs/connections/spec/page/
  const props = properties;
  props.title = document.title;
  props.path = window.location.pathname;
  props.search = window.location.search;
  props.url = window.location.href;
  props.referrer = document.referrer;

  return sendTelemetryPageRequest({
    category,
    name,
    jsonBlob: JSON.stringify(props),
  });
}
