import { Dispatch, SetStateAction, createContext } from "react";

// Coordinates listeners for keyboard inputs to prevent having >1 input listening to keyboard events at a time.
type ListenerProps = {
  listening: React.ReactNode | null;
  setListening: Dispatch<SetStateAction<string | null>>;
};

const listenerObj: ListenerProps = {
  listening: null,
  setListening: () => {},
};

const ListenerContext = createContext(listenerObj);

export default ListenerContext;
