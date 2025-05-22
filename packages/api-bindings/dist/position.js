import { PointSchema, SizeSchema } from "@aws/amazon-q-developer-cli-proto/fig";
import { sendPositionWindowRequest, sendDragWindowRequest, } from "./requests.js";
import { create } from "@bufbuild/protobuf";
// Developer Facing API
export async function isValidFrame(frame) {
    return sendPositionWindowRequest({
        size: create(SizeSchema, { width: frame.width, height: frame.height }),
        anchor: create(PointSchema, { x: frame.anchorX, y: 0 }),
        dryrun: true,
    });
}
export async function setFrame(frame) {
    return sendPositionWindowRequest({
        size: create(SizeSchema, { width: frame.width, height: frame.height }),
        anchor: create(PointSchema, {
            x: frame.anchorX,
            y: frame.offsetFromBaseline ?? 0,
        }),
    });
}
export async function dragWindow() {
    return sendDragWindowRequest({});
}
//# sourceMappingURL=position.js.map