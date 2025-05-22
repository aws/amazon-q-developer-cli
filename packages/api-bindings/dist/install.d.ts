import { NotificationResponse } from "./notifications.js";
export type Component = "dotfiles" | "ibus" | "inputMethod" | "accessibility" | "desktopEntry" | "autostartEntry" | "gnomeExtension" | "ssh";
export declare function install(component: Component): Promise<void>;
export declare function uninstall(component: Component): Promise<void>;
export declare function isInstalled(component: Component): Promise<boolean>;
export declare const installStatus: {
    subscribe: (component: "accessibility", handler: (isInstalled: boolean) => NotificationResponse | undefined) => Promise<import("./notifications.js").Subscription> | undefined;
};
