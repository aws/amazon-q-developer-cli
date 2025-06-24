import { alphaByTitle } from "@/lib/sort";
import { Action, Pref } from "@/types/preferences";
import { Setting } from "./listItem";
import { getIconFromName } from "@/lib/icons";
import { cn } from "@/lib/utils";
import { parseBackticksToCode } from "@/lib/strings";
import { Link } from "../ui/link";

export type PrefSection = {
  title: string;
  properties?: Pref[];
  actions?: Action[];
};

export type Intro = {
  title: string;
  description: string;
  link?: string;
};

function FeatureIntro({ intro }: { intro: Intro }) {
  return (
    <section className="flex flex-col justify-center p-6 gap-4 w-full gradient-q-secondary-light-alt rounded-lg items-start text-white min-h-[6.5rem]">
      <div className="flex gap-4 justify-between w-full">
        <div className="flex gap-4">
          <div className="flex-shrink-0">
            {getIconFromName(intro.title, 48)}
          </div>
          <div className="flex flex-col">
            <h1 className="font-bold text-2xl font-ember leading-none">
              {intro.title}
            </h1>
            <p className="text-base">
              {parseBackticksToCode(
                intro.description,
                "!border-white !bg-white/20 !text-white py-[1px]",
              )}
              {intro.link && (
                <Link
                  href={intro.link}
                  className="pl-1 font-medium"
                  variant="primary"
                >
                  Learn more
                </Link>
              )}
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

export function SectionHeading({
  children,
  index,
}: {
  children: React.ReactNode;
  index: number;
}) {
  return (
    <h2
      id={`subhead-${index}`}
      className="font-bold text-medium text-zinc-400 leading-none mt-2"
    >
      {children}
    </h2>
  );
}

export function UserPrefSection({
  data,
  index,
  disabled,
}: {
  data: PrefSection;
  index: number;
  disabled?: boolean;
}) {
  const list = data.properties ?? data.actions;

  return (
    <section
      className={`flex flex-col gap-4 py-4 ${
        disabled && "opacity-30 select-none"
      }`}
    >
      <SectionHeading index={index}>{data.title}</SectionHeading>

      {list?.sort(alphaByTitle).map((p, i) => {
        if (p.popular) return;

        return <Setting data={p} key={i} disabled={disabled} />;
      })}
    </section>
  );
}

export function UserPrefView({
  array,
  children,
  intro,
  className,
}: {
  array?: PrefSection[];
  children?: React.ReactNode;
  intro?: Intro;
  className?: string;
}) {
  return (
    <div className={cn("w-full flex flex-col", className)}>
      {intro && <FeatureIntro intro={intro} />}
      {children}
      {array &&
        array.map((section, i) => (
          <UserPrefSection data={section} index={i} key={i} />
        ))}
    </div>
  );
}
