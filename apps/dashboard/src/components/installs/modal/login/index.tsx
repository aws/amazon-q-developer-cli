import Lockup from "@/components/svg/logo";
import { Button } from "@/components/ui/button";
import { Auth, Internal, Native } from "@withfig/api-bindings";
import { useEffect, useState } from "react";
import Tab from "./tabs";

export default function LoginModal({ next }: { next: () => void }) {
  const [loginState, setLoginState] = useState<
    "not started" | "loading" | "logged in"
  >("not started");
  const [loginCode, setLoginCode] = useState<string | null>(null);
  const [tab, setTab] = useState<"builderId" | "iam">("builderId");

  async function handleLogin(startUrl?: string, region?: string) {
    setLoginState("loading");
    const init = await Auth.builderIdStartDeviceAuthorization({
      startUrl,
      region,
    }).catch((err) => {
      setLoginState("not started");
      console.error(err);
    });

    if (!init) return;

    setLoginCode(init.code);

    Native.open(init.url).catch((err) => {
      console.error(err);
    });

    await Auth.builderIdPollCreateToken(init)
      .then(() => {
        setLoginState("logged in");
        Internal.sendWindowFocusRequest({});
        next();
      })
      .catch((err) => {
        setLoginState("not started");
        console.error(err);
      });
  }

  useEffect(() => {
    Auth.status().then((r) => {
      console.log("auth status", r);
      setLoginState(r.authed ? "logged in" : "not started");
    });
  }, []);

  useEffect(() => {
    if (loginState !== "logged in") return;

    next();
  }, [loginState, next]);

  return (
    <div className="flex flex-col items-center gap-8 gradient-cw-secondary-light -m-10 p-4 pt-10 rounded-lg text-white">
      <div className="flex flex-col items-center gap-8">
        <Lockup />
        <h2 className="text-xl text-white font-semibold select-none leading-none font-ember tracking-tight">
          Sign in to get started
        </h2>
      </div>
      <div className="flex flex-col gap-4 text-white text-sm">
        {loginCode ? (
          <>
            <p className="text-center w-80">
              Confirm code <span className="font-bold">{loginCode}</span> in the
              login page opened in your web browser.
            </p>
            <Button
              variant="glass"
              className="self-center w-32"
              onClick={() => {
                setLoginState("not started");
                setLoginCode(null);
              }}
            >
              Back
            </Button>
          </>
        ) : (
          <Tab tab={tab} handleLogin={handleLogin} toggleTab={tab === 'builderId' ? () => setTab('iam') : () => setTab('builderId')} />
        )}
      </div>
    </div>
  );
}