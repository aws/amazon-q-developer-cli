export declare function write(path: string, contents: string): Promise<void>;
export declare function append(path: string, contents: string): Promise<void>;
export declare function read(path: string): Promise<string | null>;
export declare function list(path: string): Promise<string[]>;
export declare function destinationOfSymbolicLink(path: string): Promise<string | undefined>;
export declare function createDirectory(path: string, recursive: boolean): Promise<void>;
