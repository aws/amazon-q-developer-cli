import { Link } from "@/components/ui/link";
import feedData from "../../../../feed.json";
import { useEffect, useMemo, useState } from "react";
import { cn } from "@/lib/utils";
import Markdown, { Options as MarkdownOptions } from "react-markdown";
import { Kbd } from "@/components/ui/keystrokeInput";
import { useLocalStateZodDefault } from "@/hooks/store/useState";
import { z } from "zod";
import { NOTIFICATIONS_SEEN_STATE_KEY } from "@/lib/constants";

type FeedItemData = (typeof feedData.entries)[0];
type ChangesData = NonNullable<FeedItemData["changes"]>;

function toCapsCase(str: string) {
  return str.charAt(0).toUpperCase() + str.slice(1);
}

const MARKDOWN_OPTIONS: MarkdownOptions = {
  components: {
    a({ className, children, href }) {
      if (href && href.startsWith("/")) {
        return (
          <Link className={className} to={href}>
            {children}
          </Link>
        );
      } else {
        return (
          <Link className={className} href={href ?? ""}>
            {children}
          </Link>
        );
      }
    },
    kbd(props) {
      return <Kbd>{props.children}</Kbd>;
    },
    code({ className, children }) {
      return (
        <code
          className={cn(
            "text-[0.9em] px-1 py-px bg-zinc-50 border border-zinc-200 rounded-sm text-zinc-600 dark:border-zinc-700 dark:text-zinc-300 dark:bg-zinc-900 whitespace-nowrap",
            className,
          )}
        >
          {children}
        </code>
      );
    },
  },
};

function ChangesInner({
  changes,
  faded,
}: {
  changes: ChangesData;
  faded?: boolean;
}) {
  return (
    <div className="relative">
      <ul className="text-sm ml-4 flex flex-col gap-0.5">
        {changes.map((change) => (
          <li
            className="before:content-['•_'] flex flex-row gap-1"
            key={`${change.type}-${change.description}`}
          >
            <Markdown {...MARKDOWN_OPTIONS}>
              {`**${toCapsCase(change.type)}**: ${change.description}`}
            </Markdown>
          </li>
        ))}
      </ul>
      {faded && (
        <div className="w-full h-[60%] bg-gradient-to-t from-white dark:from-zinc-800 bottom-0 absolute pointer-events-none" />
      )}
    </div>
  );
}

function Changes({ changes }: { changes: ChangesData }) {
  const [showMore, setShowMore] = useState(false);
  const sortedChanges = useMemo(
    () => changes.sort((a, b) => a.type.localeCompare(b.type)),
    [changes],
  );

  if (changes.length > 5) {
    return (
      <>
        <ChangesInner
          changes={showMore ? sortedChanges : sortedChanges.slice(0, 5)}
          faded={!showMore}
        />
        <button
          onClick={() => setShowMore((b) => !b)}
          className="text-sm hover:opacity-70 text-dusk-600 dark:text-dusk-400"
        >
          {showMore ? "Show less ←" : "Show more →"}
        </button>
      </>
    );
  } else {
    return <ChangesInner changes={sortedChanges} />;
  }
}

function FeedItem({ item }: { item: FeedItemData }) {
  let typeName: string;
  let typeTextColor: string;
  let typeBgColor: string;
  switch (item.type) {
    case "announcement":
      typeName = "Product Announcement";
      typeTextColor = "text-[#008DFF]";
      typeBgColor = "bg-[#008DFF]";
      break;
    case "release":
      typeName = "Release Notes";
      typeTextColor = "text-dusk-600 dark:text-dusk-400";
      typeBgColor = "bg-dusk-600 dark:bg-dusk-400";
      break;
    default:
      typeName = toCapsCase(item.type ?? "");
      typeTextColor = "text-zinc-800 dark:text-zinc-200";
      typeBgColor = "bg-zinc-800 dark:bg-zinc-200";
      break;
  }

  const date = useMemo(() => {
    if (!item.date) return undefined;
    const localization = new Intl.DateTimeFormat("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
      timeZone: "UTC",
    });
    return localization.format(new Date(item.date));
  }, [item.date]);

  return (
    <div className="flex flex-row gap-4 pb-10">
      <div className="h-5 rounded-full mt-5 p-1.5 bg-white dark:bg-zinc-800 z-20 absolute" />
      <div className={cn("h-2 rounded-full mt-6 p-1.5 z-20", typeBgColor)} />
      <div className="flex flex-col items-start">
        <div className="text-xs">
          <span className={cn("font-mono font-semibold", typeTextColor)}>
            {typeName}
          </span>
          {date && (
            <span className="opacity-50 font-extralight ml-2">{date}</span>
          )}
        </div>
        <h2 className="text-xl font-bold font-ember">{item.title}</h2>
        <Markdown {...MARKDOWN_OPTIONS} className="text-sm mb-1">
          {item.description}
        </Markdown>
        {item.changes && <Changes changes={item.changes} />}
        {item.link && (
          <Link className={cn("text-sm", typeTextColor)} href={item.link}>
            Learn more →
          </Link>
        )}
      </div>
    </div>
  );
}

export default function WhatsNew() {
  const [_notifCount, setNotifCount] = useLocalStateZodDefault(
    NOTIFICATIONS_SEEN_STATE_KEY,
    z.number(),
    0,
  );
  const entries = useMemo(() => feedData.entries.filter((i) => !i?.hidden), []);

  useEffect(() => {
    setNotifCount(entries.length);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries.length]);

  return (
    <div className="relative mt-4">
      <div className="w-0.5 left-[0.3rem] mt-6 h-[calc(max(100%,100vh-3.5rem))] absolute z-0 bg-zinc-200 dark:bg-zinc-700" />
      <div className="flex flex-col item">
        {entries.map((item, itemIndex) => (
          <FeedItem item={item} key={itemIndex} />
        ))}
      </div>
    </div>
  );
}
