type Property = string | boolean | number | null;
export declare function track(event: string, properties: Record<string, Property>): Promise<void>;
export declare function page(category: string, name: string, properties: Record<string, Property>): Promise<void>;
export {};
