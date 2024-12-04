import { UserPrefView } from "@/components/preference/list";
import { Code } from "@/components/text/code";
import { Kbd } from "@/components/ui/keystrokeInput";
import { Terminal } from "@/components/ui/terminal";
import settings, { intro } from "@/data/inline";
import inlineDemo from "@assets/images/inline_demo.gif";
import { useEffect, useState } from "react";
import { Codewhisperer } from "@aws/amazon-q-developer-cli-api-bindings";
import { useLocalStateZod } from "@/hooks/store/useState";
import { z } from "zod";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
import { Link } from "@/components/ui/link";

const CUSTOMIZATIONS_STATE_KEY = "api.selectedCustomization";

function CustomizationsSelector() {
  const [customizations, setCustomizations] =
    useState<Codewhisperer.Customization[]>();

  const [selectedCustomizations, setSelectedCustomizations] = useLocalStateZod(
    CUSTOMIZATIONS_STATE_KEY,
    z
      .object({
        arn: z.string(),
        name: z.string().nullish(),
        description: z.string().nullish(),
      })
      .nullish(),
  );

  useEffect(() => {
    Codewhisperer.listCustomizations().then((c) => setCustomizations(c));
  }, []);

  const name = selectedCustomizations?.name ?? undefined;

  if (!customizations || customizations.length === 0) {
    return <></>;
  }

  return (
    <div className="flex p-4 pl-0 gap-4">
      <div className="flex-none w-12"></div>
      <div className="flex flex-col gap-1">
        <h3 className="leading-none">
          <span className="font-medium">Customization</span>
          <Link
            href="https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/customizations.html"
            className="ml-2 text-sm font-light"
          >
            Learn More
          </Link>
        </h3>
        <div className="pt-1">
          <Select
            onValueChange={(newName) => {
              const c = customizations.find((c) => c.name === newName);
              setSelectedCustomizations(c);
            }}
            value={name}
          >
            <SelectTrigger className="min-w-60 w-fit">
              <span className="whitespace-nowrap">{name ?? "None"} </span>
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {[
                  { name: "None", description: undefined },
                  ...customizations,
                ].map((c, i) => {
                  return (
                    <SelectItem value={c.name ?? ""} key={i}>
                      <span>{c.name}</span>
                      {c.description && (
                        <>
                          <br />
                          <span className="text-xs opacity-70">
                            {c.description}
                          </span>
                        </>
                      )}
                    </SelectItem>
                  );
                })}
              </SelectGroup>
            </SelectContent>
          </Select>
        </div>
      </div>
    </div>
  );
}

export default function Page() {
  return (
    <div className="flex flex-col mb-10">
      <UserPrefView intro={intro} />
      <div className="flex flex-col gap-4 py-4">
        <h2 className="font-bold text-medium text-zinc-400 mt-2 leading-none">
          How To
        </h2>
        <p className="font-light leading-snug">
          Receive AI-generated completions as you type. Press <Kbd>→</Kbd> to
          accept. Currently <Code>zsh</Code> only.
        </p>
      </div>
      <Terminal title={"Inline shell completions"} className="my-2">
        <Terminal.Tab>
          <img
            src={inlineDemo}
            alt="Inline shell completion demo"
            className="rounded-xl"
          />
        </Terminal.Tab>
      </Terminal>
      <UserPrefView array={settings} />
      <CustomizationsSelector />
    </div>
  );
}
