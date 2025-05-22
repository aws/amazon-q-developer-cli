export declare function isValidFrame(frame: {
    width: number;
    height: number;
    anchorX: number;
}): Promise<import("@aws/amazon-q-developer-cli-proto/fig").PositionWindowResponse>;
export declare function setFrame(frame: {
    width: number;
    height: number;
    anchorX: number;
    offsetFromBaseline: number | undefined;
}): Promise<import("@aws/amazon-q-developer-cli-proto/fig").PositionWindowResponse>;
export declare function dragWindow(): Promise<void>;
