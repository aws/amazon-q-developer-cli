import React, { useState } from "react";

type DivProps = React.HTMLAttributes<HTMLDivElement>;

export const Notification = ({
  title,
  description,
  localStorageKey,
  show,
  ...props
}: Omit<Omit<DivProps, "title">, "children"> & {
  title: React.ReactNode;
  description: React.ReactNode;
  localStorageKey: string;
  show: boolean;
}) => {
  const LOCAL_STORAGE_IDENTIFIER = `notification_dismissed.${localStorageKey}`;

  const [hasDismissed, setHasDismissed] = useState(
    localStorage.getItem(LOCAL_STORAGE_IDENTIFIER) === "true",
  );

  const onDismiss = () => {
    setHasDismissed(true);
    localStorage.setItem(LOCAL_STORAGE_IDENTIFIER, "true");
  };

  if (hasDismissed || !show) {
    return null;
  }

  return (
    <div {...props} className="notification">
      <div className="flex-grow">
        <div className="small-text flex items-center font-semibold">
          {title}
        </div>
        <div className="smaller-text">{description}</div>
      </div>
      <button
        onClick={onDismiss}
        type="button"
        className="dismiss"
        aria-label="Dismiss Notification"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 20 20"
          fill="currentColor"
          aria-hidden="true"
          className="h-6 w-6"
        >
          <path
            d="M6.28
                5.22a.75.75
                0
                00-1.06
                1.06L8.94
                10l-3.72
                3.72a.75.75
                0
                101.06
                1.06L10
                11.06l3.72
                3.72a.75.75
                0
                101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z"
          />
        </svg>
      </button>
    </div>
  );
};
