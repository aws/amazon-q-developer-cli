export type Param = string | number | Uint8Array | null;
export declare function query(sql: string, params?: Param[]): Promise<Array<Record<string, unknown>>>;
