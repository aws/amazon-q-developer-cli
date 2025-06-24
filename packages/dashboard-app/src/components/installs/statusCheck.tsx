import { InstallCheckWithInstallKey } from "@/types/preferences";
import { Install } from "@aws/amazon-q-developer-cli-api-bindings";
import { useEffect, useState } from "react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "../ui/collapsible";
import { Check, ChevronDown, X } from "lucide-react";
import { Button } from "../ui/button";
import { useSetting, useStatusCheck } from "@/hooks/store";

export default function StatusCheck({
  check,
}: {
  check: InstallCheckWithInstallKey;
}) {
  const [expanded, setExpanded] = useState(false);
  const [status, refreshStatus] = useStatusCheck(check.installKey);
  const [err, setErr] = useState("");
  const [launchOnStartup] = useSetting("app.launchOnStartup");

  function fixInstall() {
    Install.install(check.installKey)
      .then(() => {
        // Default behavior for launchOnStartup should be true, hence check for undefined.
        if (
          check.installKey === "desktopEntry" &&
          (launchOnStartup === undefined || launchOnStartup === true)
        ) {
          Install.install("autostartEntry")
            .then(() => {
              refreshStatus();
            })
            .catch((e) => {
              console.error(e);
              setErr(e.toString());
            });
        }

        refreshStatus();
      })
      .catch((e) => {
        console.error(e);
        setErr(e.toString());
      });
  }

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  return (
    <Collapsible
      className="flex gap-4 self-stretch"
      open={expanded}
      onOpenChange={setExpanded}
    >
      <CollapsibleTrigger asChild className="mt-5 flex-none">
        <ChevronDown
          className={`h-5 w-5 ${
            expanded ? "rotate-0" : "-rotate-90"
          } cursor-pointer text-zinc-400`}
        />
      </CollapsibleTrigger>
      <div className="flex flex-col border-b-[1px] border-zinc-200 dark:border-zinc-600 py-4 flex-auto gap-1">
        <div className="flex gap-2 items-center">
          <h2 className="font-medium text-lg select-none">{check.title}</h2>
          {status !== undefined &&
            (status ? (
              <Check className="h-5 w-5 text-green-600 dark:text-green-400" />
            ) : (
              <X className="h-5 w-5 text-red-600 dark:text-red-400" />
            ))}
        </div>
        <CollapsibleContent className="flex flex-col gap-2 text-base font-light text-zinc-500 dark:text-zinc-400 items-start leading-tight">
          {check.description.map((d, i) => (
            <p key={i}>{d}</p>
          ))}
          <Button
            onClick={fixInstall}
            disabled={status}
            className="disabled:bg-zinc-400 dark:disabled:bg-zinc-600 dark:disabled:border-zinc-400 dark:bg-dusk-600 dark:border dark:border-dusk-400 dark:hover:border-dusk-700 dark:hover:bg-dusk-800 select-none h-auto py-2 px-6 mt-1"
          >
            {status ? "Enabled" : check.action}
          </Button>
          {status !== undefined && status == false && err !== "" && (
            <pre className="text-sm text-zinc-900 dark:text-zinc-200 bg-red-100 dark:bg-red-950 border-red-400 border flex items-center justify-center leading-none p-2 px-2 rounded overflow-x-auto max-w-full whitespace-pre-wrap">
              {err}
            </pre>
          )}
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
}
