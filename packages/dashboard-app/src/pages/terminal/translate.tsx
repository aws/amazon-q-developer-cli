import { UserPrefView } from "@/components/preference/list";
import settings, { intro } from "@/data/translate";

export default function Page() {
  return <UserPrefView array={settings} intro={intro} />;
}
