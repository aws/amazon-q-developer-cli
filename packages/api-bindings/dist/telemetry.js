import { TelemetryPropertySchema, } from "@aws/amazon-q-developer-cli-proto/fig";
import { sendTelemetryPageRequest, sendTelemetryTrackRequest, } from "./requests.js";
import { create } from "@bufbuild/protobuf";
export function track(event, properties) {
    // convert to internal type 'TelemetryProperty'
    const props = Object.keys(properties).reduce((array, key) => {
        const entry = create(TelemetryPropertySchema, {
            key,
            value: JSON.stringify(properties[key]),
        });
        array.push(entry);
        return array;
    }, []);
    return sendTelemetryTrackRequest({
        event,
        properties: props,
        jsonBlob: JSON.stringify(properties),
    });
}
export function page(category, name, properties) {
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
//# sourceMappingURL=telemetry.js.map