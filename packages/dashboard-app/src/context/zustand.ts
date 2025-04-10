import { Store } from "@/lib/store";
import { createContext } from "react";

export const StoreContext = createContext<Store | null>(null);
