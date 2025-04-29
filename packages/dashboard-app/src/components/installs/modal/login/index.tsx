import Lockup from "@/components/svg/logo";
import { Button } from "@/components/ui/button";
import {
  Auth,
  Internal,
  Native,
} from "@aws/amazon-q-developer-cli-api-bindings";
import { useEffect, useState } from "react";
import Tab, { ProfileTab } from "./tabs";
import { useLocalStateZodDefault } from "@/hooks/store/useState";
import { z } from "zod";
import { Link } from "@/components/ui/link";
import { Q_MIGRATION_URL } from "@/lib/constants";
import { useAuth, useAuthRequest, useRefreshAuth } from "@/hooks/store/useAuth";

export default function LoginModal({ next }: { next: () => void }) {
  const midway = window?.fig?.constants?.midway ?? false;

  const [loginState, setLoginState] = useState<
    "not started" | "loading" | "logged in"
  >("not started");
  const [tab, setTab] = useLocalStateZodDefault<"builderId" | "iam">(
    "dashboard.loginTab",
    z.enum(["builderId", "iam"]),
    midway ? "iam" : "builderId",
  );

  // Since PKCE requires the ability to open a browser, we also support falling
  // back to device code in case of an error.
  const [loginMethod, setLoginMethod] = useState<"pkce" | "deviceCode">("pkce");

  // used for pkce
  const [currAuthRequestId, setAuthRequestId] = useAuthRequest();
  const [pkceTimedOut, setPkceTimedOut] = useState(false);
  useEffect(() => {
    if (pkceTimedOut) return;
    if (loginMethod === "pkce" && loginState === "loading") {
      const timer = setTimeout(() => setPkceTimedOut(true), 4000);
      return () => clearTimeout(timer);
    }
  }, [loginMethod, loginState, pkceTimedOut]);

  // used for device code
  const [loginCode, setLoginCode] = useState<string | null>(null);
  const [loginUrl, setLoginUrl] = useState<string | null>(null);
  const [copyToClipboardText, setCopyToClipboardText] = useState<
    "Copy to clipboard" | "Copied!"
  >("Copy to clipboard");
  const [error, setError] = useState<string | null>(null);
  const [completedOnboarding] = useLocalStateZodDefault(
    "desktop.completedOnboarding",
    z.boolean(),
    false,
  );
  const [showProfileTab, setShowProfileTab] = useState(false);
  const auth = useAuth();
  const refreshAuth = useRefreshAuth();

  useEffect(() => {
    // Reset the auth request id so that we don't present the "OAuth cancelled" error
    // to the user.
    setAuthRequestId("");
  }, [loginMethod, setAuthRequestId]);

  async function handleLogin(startUrl?: string, region?: string) {
    if (loginMethod === "pkce") {
      handlePkceAuth(startUrl, region);
    } else {
      handleDeviceCodeAuth(startUrl, region);
    }
  }

  async function handlePkceAuth(issuerUrl?: string, region?: string) {
    setLoginState("loading");
    setPkceTimedOut(false);
    setError(null);
    // We need to reset the auth request state before attempting, otherwise
    // an expected auth request cancellation will be presented to the user
    // as an error.
    setAuthRequestId(undefined);
    const init = await Auth.startPkceAuthorization({
      issuerUrl,
      region,
    }).catch((err) => {
      setLoginState("not started");
      setError(err.message);
      console.error(err);
    });

    if (!init) return;
    setAuthRequestId(init.authRequestId);

    Native.open(init.url).catch((err) => {
      console.error(err);
      setError(
        "Failed to open the browser. As an alternative, try logging in with device code.",
      );
    });

    await Auth.finishPkceAuthorization({
      authRequestId: init.authRequestId,
    })
      .then(() => {
        Internal.sendWindowFocusRequest({});
        if (tab == "iam") {
          setShowProfileTab(true);
        } else {
          refreshAuth();
          next();
        }
      })
      .catch((err) => {
        // If this promise was originally for some older request attempt,
        // then we should just ignore the error.
        if (currAuthRequestId() === init.authRequestId) {
          setLoginState("not started");
          setError(err.message);
          console.error(err);
        }
      });
  }

  async function handleDeviceCodeAuth(startUrl?: string, region?: string) {
    setLoginState("loading");
    setError(null);
    setLoginUrl(null);
    setCopyToClipboardText("Copy to clipboard");
    await Auth.cancelPkceAuthorization().catch((err) => {
      console.error(err);
    });
    const init = await Auth.builderIdStartDeviceAuthorization({
      startUrl,
      region,
    }).catch((err) => {
      setLoginState("not started");
      setLoginCode(null);
      setError(err.message);
      console.error(err);
    });

    if (!init) return;

    setLoginCode(init.code);
    setLoginUrl(init.url);

    await Auth.builderIdPollCreateToken(init)
      .then(() => {
        setLoginState("logged in");
        Internal.sendWindowFocusRequest({});
        refreshAuth();
        next();
      })
      .catch((err) => {
        setLoginState("not started");
        setLoginCode(null);
        setError(err.message);
        console.error(err);
      });
  }

  useEffect(() => {
    setLoginState(auth.authed ? "logged in" : "not started");
  }, [auth]);

  useEffect(() => {
    if (loginState !== "logged in" || showProfileTab) return;
    next();
  }, [loginState, showProfileTab, next]);

  return showProfileTab ? (
    <ProfileTab
      next={() => {
        refreshAuth();
        setLoginState("logged in");
      }}
      back={() => {
        setLoginState("not started");
        setShowProfileTab(false);
      }}
    />
  ) : (
    <div className="flex flex-col items-center gap-8 gradient-q-secondary-light -m-10 pt-10 p-4 rounded-lg text-white">
      <div className="flex flex-col items-center gap-8">
        <Lockup />
        {!completedOnboarding && (
          <h2 className="text-xl text-white font-semibold select-none leading-none font-ember tracking-tight">
            Sign in to get started
          </h2>
        )}
        {completedOnboarding && tab == "builderId" && (
          <div className="text-center flex flex-col">
            <div className="font-ember font-bold">
              CodeWhisperer is now Amazon Q
            </div>
            <Link href={Q_MIGRATION_URL} className="text-sm">
              Read the announcement blog post
            </Link>
          </div>
        )}
      </div>
      {error && (
        <div className="flex flex-col items-center gap-2 w-full bg-red-200 border border-red-600 rounded py-2 px-2">
          <p className="text-black dark:text-white font-semibold text-center">
            Failed to login
          </p>
          <p className="text-black dark:text-white text-center">{error}</p>
          {loginMethod === "pkce" && loginState === "loading" && (
            <Button
              variant="ghost"
              className="self-center mx-auto text-black hover:bg-white/40"
              onClick={() => {
                setLoginMethod("deviceCode");
                setLoginState("not started");
                setError(null);
              }}
            >
              Login with Device Code
            </Button>
          )}
        </div>
      )}

      <div className="flex flex-col items-center gap-4 text-white text-sm">
        {loginState === "loading" && loginMethod === "pkce" ? (
          <>
            <p className="text-center w-80">
              Waiting for authentication in the browser to complete...
            </p>
            {pkceTimedOut && (
              <div className="text-center w-80">
                <p>Browser not opening?</p>
                <Button
                  variant="ghost"
                  className="h-auto p-1 px-2 hover:bg-white/20 hover:text-white italic"
                  onClick={() => {
                    setLoginMethod("deviceCode");
                    setLoginState("not started");
                  }}
                >
                  Try authenticating with device code
                </Button>
              </div>
            )}
            <Button
              variant="glass"
              className="self-center w-32"
              onClick={() => {
                setLoginState("not started");
              }}
            >
              Back
            </Button>
          </>
        ) : loginState === "loading" &&
          loginMethod === "deviceCode" &&
          loginCode &&
          loginUrl ? (
          <>
            <p className="text-center w-80">
              Confirm code <span className="font-bold">{loginCode}</span> in the
              login page at the following link:
            </p>
            <p className="text-center">{loginUrl}</p>
            <Button
              variant="ghost"
              className="h-auto p-1 px-2 hover:bg-white/20 hover:text-white"
              onClick={() => {
                navigator.clipboard.writeText(loginUrl);
                setCopyToClipboardText("Copied!");
              }}
            >
              {copyToClipboardText}
            </Button>
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
          <Tab
            tab={tab}
            handleLogin={handleLogin}
            toggleTab={
              tab === "builderId"
                ? () => setTab("iam")
                : () => setTab("builderId")
            }
            signInText={completedOnboarding ? "Log back in" : "Sign in"}
          />
        )}
      </div>
    </div>
  );
}
