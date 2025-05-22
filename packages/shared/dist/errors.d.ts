export declare const createErrorInstance: (name: string) => {
    new (message?: string): {
        name: string;
        message: string;
        stack?: string;
        cause?: unknown;
    };
};
