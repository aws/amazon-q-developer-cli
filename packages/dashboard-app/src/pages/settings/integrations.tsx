import { UserPrefView } from "@/components/preference/list";
import settings from "@/data/integrations";
import { parseBackticksToCode } from "@/lib/strings";
import { CLI_BINARY_NAME } from "@/lib/constants";

export default function Page() {
  const setupString = `Just run \`${CLI_BINARY_NAME} integrations install input-method\` then restart your computer to try it out.`;

  return (
    <>
      <UserPrefView
        array={settings}
        className="w-auto self-start [&>section]:grid [&>section]:grid-cols-1 [&>section]:md:grid-cols-2 [&>section]:xl:grid-cols-3 [&>section]:2xl:grid-cols-4 [&>section>h2]:col-span-full"
      />
      <div className="flex flex-col p-4 gap-1 rounded-lg bg-zinc-50 dark:bg-zinc-900 border border-zinc-100 dark:border-zinc-700">
        <h2 className="font-bold font-ember text-lg items-center flex">
          <span className="uppercase py-1 px-2 bg-cyan-500 font-mono text-white text-xs mr-2 rounded-sm">
            Beta
          </span>
          <span className="leading-none">
            Want support for JetBrains, Alacritty, Kitty, and Ghostty?
          </span>
        </h2>
        <p>{parseBackticksToCode(setupString)}</p>
      </div>
    </>
  );
}
