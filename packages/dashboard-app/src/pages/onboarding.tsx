import autocompleteDemo from "@assets/images/autocomplete_demo.gif";
import translateDemo from "@assets/images/translate_demo.gif";
import chatDemo from "@assets/images/chat_demo.gif";
import { Link } from "@/components/ui/link";
import { QLogo } from "@/components/svg/icons";
import { AUTOCOMPLETE_SSH_WIKI_URL, Q_MIGRATION_URL } from "@/lib/constants";
import { Terminal } from "@/components/ui/terminal";

export default function Page() {
  return (
    <div className="flex flex-col items-start gap-4">
      <div className="flex flex-col">
        <h1 className="text-2xl font-bold dark:text-zinc-400">
          Getting started
        </h1>
      </div>
      <section className="flex flex-col p-6 gap-4 w-full gradient-q-secondary-light-alt rounded-lg items-start text-white">
        <div className="flex flex-row gap-4 w-full items-center">
          <QLogo size={42} />
          <div className="flex flex-col gap-1">
            <h1 className="font-bold text-xl font-ember leading-none">
              CodeWhisperer is now Amazon Q
            </h1>
            <p className="text-base leading-tight">
              <Link
                href={Q_MIGRATION_URL}
                className="font-medium"
                variant="primary"
              >
                Read the announcement blog post
              </Link>
            </p>
          </div>
        </div>
      </section>
      <div className="flex flex-col gap-2 w-full border bg-zinc-50 border-zinc-200 dark:bg-zinc-900 dark:border-zinc-600 rounded-lg p-4 mb-4 overflow-x-auto">
        <h2 className="font-bold font-ember tracking-tight text-lg dark:text-zinc-300">
          Launch a new shell session to start using autocomplete!
        </h2>
        <ol className="flex flex-col gap-0.5 text-zinc-600 dark:text-zinc-400">
          <li>
            <span className="text-sm flex items-baseline gap-1">
              <span className="font-semibold">Not working?</span>
              <span>Check out</span>
              <Link to="/help" variant="blue">
                Help & support
              </Link>
            </span>
          </li>
          <li>
            <span className="text-sm">
              <span className="font-semibold">Want to customize settings?</span>{" "}
              Use the tabs in the sidebar.
            </span>
          </li>
          <li>
            <div className="text-sm flex flex-row items-center gap-1 flex-wrap">
              <div className="w-10.5">
                <div className="rounded-full px-2 py-0.5 bg-rose-400/50 dark:bg-rose-600/50 border border-rose-400/80 dark:border-rose-600/80 relative motion-safe:animate-shine before:absolute before:inset-0 before:transform before:-translate-x-full motion-safe:before:animate-shimmer before:bg-gradient-to-r before:filter before:from-rose-600/0 before:via-rose-600/40 before:to-bg-rose-600/0 isolate overflow-hidden mb-0">
                  <span className="relative z-10 flex items-center">
                    <p className="tracking-normal text-xs font-medium text-black dark:text-white">
                      New
                    </p>
                  </span>
                </div>
              </div>
              <span className="font-semibold whitespace-nowrap">
                Want to use SSH?
              </span>
              <span className="whitespace-pre">
                See{" "}
                <Link href={AUTOCOMPLETE_SSH_WIKI_URL} variant="blue">
                  Using CLI Autocomplete on a remote machine
                </Link>{" "}
                for more information.
              </span>
            </div>
          </li>
        </ol>
      </div>
      <Terminal tabNames={["Autocomplete", "Translation", "Chat"]}>
        <Terminal.Tab>
          <img src={autocompleteDemo} alt="autocomplete demo" />
        </Terminal.Tab>
        <Terminal.Tab>
          <img src={translateDemo} alt="translation demo" />
        </Terminal.Tab>
        <Terminal.Tab>
          <img src={chatDemo} alt="chat demo" />
        </Terminal.Tab>
      </Terminal>
    </div>
  );
}
