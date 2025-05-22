export declare const LoginShellError: new (message?: string) => {
    name: string;
    message: string;
    stack?: string;
    cause?: unknown;
};
export declare const executeLoginShell: ({ command, executable, shell, timeout, }: {
    command: string;
    executable?: string;
    shell?: string;
    timeout?: number;
}) => Promise<string>;
export declare const executeCommand: Fig.ExecuteCommandFunction;
