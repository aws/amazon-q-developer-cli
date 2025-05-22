export declare function run({ executable, args, environment, workingDirectory, terminalSessionId, timeout, }: {
    executable: string;
    args: string[];
    environment?: Record<string, string | undefined>;
    workingDirectory?: string;
    terminalSessionId?: string;
    timeout?: number;
}): Promise<import("@aws/amazon-q-developer-cli-proto/fig").RunProcessResponse>;
