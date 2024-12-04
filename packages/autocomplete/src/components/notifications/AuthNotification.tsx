import { useEffect, useMemo, useState } from "react";
import qIcon from "../../assets/icons/q.png";
import { Auth, State } from "@aws/amazon-q-developer-cli-api-bindings";
import { CLI_NAME, PRODUCT_NAME } from "../../consts";

// 36 hours
const DISMISSED_DURATION_MS = 129600000;

const DISMISSED_KEY = "autocomplete.authNotification.dismissedAt";

function AuthNotification() {
  const [dismissedAt, setDismissedAt] = useState<Date | undefined>(undefined);
  const [isAuthed, setIsAuthed] = useState(true);

  useEffect(() => {
    Auth.status().then(({ authed }) => {
      setIsAuthed(authed);
    });
  }, [setIsAuthed]);

  useEffect(() => {
    State.get(DISMISSED_KEY).then((value) => {
      setDismissedAt(new Date(value));
    });
  }, [setDismissedAt]);

  const isDismissed = useMemo(
    () =>
      isAuthed ||
      (dismissedAt &&
        dismissedAt > new Date(Date.now() - DISMISSED_DURATION_MS)),
    [isAuthed, dismissedAt],
  );

  if (isDismissed) {
    return null;
  }

  return (
    <div className="mx-[3px] my-[3px] font-sans text-neutral-300 bg-gradient-to-tr from-[#008DFF] via-[#4A00C8] to-[#39127D] rounded-[4px] flex flex-row items-center justify-between gap-2 py-3 px-2">
      <div className="font-medium flex flex-row items-center gap-1.5">
        <img
          src={qIcon}
          alt={`${PRODUCT_NAME} Icon`}
          className="w-5 h-5 invert shrink-0"
        />
        <div className="text-[0.8em] whitespace text-left">
          Run{" "}
          <span className="font-mono font-bold text-white">
            {CLI_NAME} login
          </span>{" "}
          to log back into {PRODUCT_NAME}
        </div>
      </div>
      <button
        className="text-[0.8em] font-light italic mr-1 whitespace-nowrap"
        aria-label="Dismiss"
        onClick={() => {
          setDismissedAt(new Date());
          State.set(DISMISSED_KEY, new Date().toISOString());
        }}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 20 20"
          fill="currentColor"
          className="size-5"
        >
          <path d="M6.28 5.22a.75.75 0 0 0-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 1 0 1.06 1.06L10 11.06l3.72 3.72a.75.75 0 1 0 1.06-1.06L11.06 10l3.72-3.72a.75.75 0 0 0-1.06-1.06L10 8.94 6.28 5.22Z" />
        </svg>
      </button>
    </div>
  );
}

export default AuthNotification;
