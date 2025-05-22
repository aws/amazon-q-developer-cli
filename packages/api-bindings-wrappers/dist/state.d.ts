export declare enum States {
    DEVELOPER_API_HOST = "developer.apiHost",
    DEVELOPER_AE_API_HOST = "developer.autocomplete-engine.apiHost",
    IS_FIG_PRO = "user.account.is-fig-pro"
}
export type LocalStateMap = Partial<{
    [States.DEVELOPER_API_HOST]: string;
    [States.DEVELOPER_AE_API_HOST]: string;
    [States.IS_FIG_PRO]: boolean;
} & {
    [key in States]: unknown;
}>;
export type LocalStateSubscriber = {
    initial?: (initialState: LocalStateMap) => void;
    changes: (oldState: LocalStateMap, newState: LocalStateMap) => void;
};
export declare class State {
    private static state;
    private static subscribers;
    static current: () => Partial<{
        "developer.apiHost": string;
        "developer.autocomplete-engine.apiHost": string;
        "user.account.is-fig-pro": boolean;
    } & {
        "developer.apiHost": unknown;
        "developer.autocomplete-engine.apiHost": unknown;
        "user.account.is-fig-pro": unknown;
    }>;
    static subscribe: (subscriber: LocalStateSubscriber) => void;
    static unsubscribe: (subscriber: LocalStateSubscriber) => void;
    static watch: () => Promise<void>;
}
