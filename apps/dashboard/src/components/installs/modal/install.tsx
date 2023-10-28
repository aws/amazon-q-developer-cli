import { Code } from "@/components/text/code";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { useStatusCheck } from "@/hooks/store/useStatusCheck";
import { InstallCheck } from "@/types/preferences";
import { Install } from "@withfig/api-bindings";
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

  useEffect(() => {
    if (timeElapsed) return;

    const timer = setTimeout(() => setTimeElapsed(true), 5000);
    return () => clearTimeout(timer);
  }, [timeElapsed]);

  useEffect(() => {
    if (!isInstalled) return;

    next();
  }, [isInstalled, next]);

  function handleInstall(key: InstallCheck["installKey"]) {
    if (!key) return;

    if (checking) {
      next();
      return;
    }

    Install.install(key)
      .then(() => setChecking(true))
      .catch((e) => console.error(e));
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-between items-baseline">
        <h2 className="font-medium text-lg select-none leading-none">
          {check.title}
        </h2>
        {timeElapsed && (
          <button className={"text-xs text-black/50"} onClick={skip}>
            skip
          </button>
        )}
      </div>
      <div className="flex flex-col gap-2 text-base font-light text-zinc-500 select-none items-start leading-tight">
        {check.description.map((d, i) => (
          <p key={i} className="text-sm">
            {d}
          </p>
        ))}
        {check.image && (
          <img
            src={check.image}
            className="h-auto w-full min-h-40 rounded-sm bg-zinc-200 border border-zinc-300"
          />
        )}
      </div>
      <div className="flex flex-col gap-1">
        <Button onClick={() => handleInstall(check.installKey)}>
          {checking ? "Continue" : check.action}
        </Button>
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
              <ul className="flex flex-col gap-4 py-4">
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