import { SectionHeading, UserPrefView } from "@/components/preference/list";
import { Setting } from "@/components/preference/listItem";
import settings, { intro } from "@/data/autocomplete";
import { alphaByTitlePrioritized } from "@/lib/sort";

export default function Page() {
  const popular = settings
    .map((s) => {
      return s.properties?.filter((p) => p.popular) ?? [];
    })
    .flat();

  return (
    <UserPrefView array={settings} intro={intro}>
      <section className="flex flex-col pt-4">
        <SectionHeading index={settings.length}>Popular</SectionHeading>
        {popular.sort(alphaByTitlePrioritized).map((p, i) => (
          <Setting data={p} key={i} />
        ))}
      </section>
    </UserPrefView>
  );
}
