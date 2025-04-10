import { Code } from "@/components/text/code";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { useStatusCheck } from "@/hooks/store/useStatusCheck";
import { InstallCheck } from "@/types/preferences";
import { Install } from "@aws/amazon-q-developer-cli-api-bindings";
import { ChevronDown } from "lucide-react";
import { useEffect, useState } from "react";

type installKey = "dotfiles" | "accessibility" | "inputMethod";

export default function InstallModal({
  check,
  skip,
  next,
}: {
  check: InstallCheck;
  skip: () => void;
  next: () => void;
}) {
  const [explainerOpen, setExplainerOpen] = useState(false);
  const [isInstalled] = useStatusCheck(check.installKey as installKey);
  const [timeElapsed, setTimeElapsed] = useState(false);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (timeElapsed) return;

    const timer = setTimeout(() => setTimeElapsed(true), 10000);
    return () => clearTimeout(timer);
  }, [check, timeElapsed]);

  // Skip already installed checks.
  useEffect(() => {
    if (!isInstalled) return;
    next();
  }, [isInstalled, next]);

  // Reset local state on new check.
  useEffect(() => {
    setExplainerOpen(false);
    setTimeElapsed(false);
    setChecking(false);
    setError(null);
  }, [check, setExplainerOpen, setTimeElapsed, setChecking, setError]);

  function handleInstall(key: InstallCheck["installKey"]) {
    if (!key) return;

    if (checking) {
      next();
      return;
    }

    setChecking(true);

    Install.install(key)
      .then(() => {
        if (key === "accessibility") {
          // The accessibility request is asynchronous - this only opens the accessibility
          // panel. We get notified when accessibility has been enabled in a separate subscription,
          // so we should return early here.
          return;
        }
        if (key === "desktopEntry") {
          // The app should autostart by default, thus immediately install the autostart entry.
          Install.install("autostartEntry")
            .then(() => {
              setChecking(false);
              next();
            })
            .catch((e) => {
              setError(
                `Failed to install the local autostart entry: ${e.message}`,
              );
              setTimeElapsed(true);
              console.error(e);
            });
        } else {
          setChecking(false);
          next();
        }
      })
      .catch((e) => {
        setError(e.message);
        setTimeElapsed(true);
        console.error(e);
      });
  }

  return (
    <div className="flex flex-col gap-4">
      <h2 className="font-medium text-lg select-none leading-none dark:text-zinc-300">
        {check.title}
      </h2>
      <div className="flex flex-col gap-2 text-base font-light text-zinc-500 dark:text-zinc-300 select-none items-start leading-tight">
        {check.description.map((d, i) => (
          <p key={i} className="text-sm">
            {d}
          </p>
        ))}
        {check.image && (
          <img
            src={check.image}
            className="h-auto w-full min-h-20 rounded-sm bg-zinc-200 dark:bg-zinc-900 border border-zinc-300 dark:border-zinc-700"
          />
        )}
      </div>
      <div className="flex flex-col gap-2">
        {error && (
          <p className="text-xs text-red-500 dark:text-red-400">{error}</p>
        )}
        <Button
          disabled={checking}
          onClick={() => handleInstall(check.installKey)}
          className="dark:bg-dusk-600"
        >
          {checking ? check.actionWaitingText || "Waiting..." : check.action}
        </Button>
        {(timeElapsed || check.skippable) && (
          <button
            className={"text-xs text-zinc-400 dark:text-zinc-400 self-center"}
            onClick={skip}
          >
            skip
          </button>
        )}
        {check.explainer && (
          <Collapsible open={explainerOpen} onOpenChange={setExplainerOpen}>
            <CollapsibleTrigger asChild className="text-zinc-400">
              <div className="flex items-center">
                <ChevronDown
                  className={`h-3 w-3 ${
                    explainerOpen ? "rotate-0" : "-rotate-90"
                  } cursor-pointer text-zinc-400`}
                />
                <span className="text-xs text-zinc-400 select-none cursor-pointer">
                  {check.explainer.title}
                </span>
              </div>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <ul className="flex flex-col gap-4 py-4 dark:text-zinc-300">
                {check.explainer.steps.map((step, i) => {
                  return (
                    <li key={i} className="flex items-baseline gap-2 text-xs">
                      <span>{i + 1}.</span>
                      <p className="flex flex-wrap gap-[0.25em]">
                        {step.map((str, i) => {
                          switch (str.tag) {
                            case "code":
                              return <Code key={i}>{str.content}</Code>;
                            default:
                            case "span":
                              return <span key={i}>{str.content}</span>;
                          }
                        })}
                      </p>
                    </li>
                  );
                })}
              </ul>
            </CollapsibleContent>
          </Collapsible>
        )}
      </div>
    </div>
  );
}
