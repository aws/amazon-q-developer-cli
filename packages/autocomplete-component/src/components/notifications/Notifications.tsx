import { useAutocompleteStore } from "../../state";
// import AuthNotification from "./AuthNotification";

export const Notifications = () => {
  const { suggestions } = useAutocompleteStore();

  if (!suggestions || suggestions.length === 0) return null;

  // TODO: make sure no more than 1 notification is shown at a time
  // return <AuthNotification />;
  return <></>;
};
