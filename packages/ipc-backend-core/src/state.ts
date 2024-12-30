import type { JsonValue } from "type-fest"

export interface State {
    getAll(): Promise<JsonValue>
    get(key: string): Promise<JsonValue>
    set(key: string, value: JsonValue): Promise<void>
    remove(key: string): Promise<void>
}

