import React from "react";

export function useCountdown(seconds: number, onEnd: () => unknown) {
  const [remaining, setRemaining] = React.useState(seconds);

  React.useEffect(() => {
    function tick() {
      setRemaining(remaining - 1);
    }

    const countdown = setInterval(tick, 1000);

    if (remaining < 0) {
      clearInterval(countdown);
      onEnd();
    }

    return () => clearInterval(countdown);
  }, [remaining, onEnd]);

  return remaining;
}
