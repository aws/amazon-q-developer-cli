import { UserPrefSection } from "@/components/preference/list";
import { Setting } from "@/components/preference/listItem";
import settings from "@/data/autocomplete";
import { alphaByTitle } from "@/lib/sort";


export default function Page() {
  const popular = (settings as any).map((s: any) => {
   return s.properties.filter((p: any) => p.popular)
  }).flat()
  return (
    <>
      <section className="flex flex-col">
        <h1 id={`subhead-popular`} className="font-bold text-2xl leading-none mt-2">Popular</h1>
        {popular.sort(alphaByTitle).map((p: any, i: number) => <Setting data={p} key={i} />)}
      </section>
      {settings.map((section, i) => <UserPrefSection data={section} index={i} key={i} />)}
    </>
  );
}