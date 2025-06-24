import { UserPrefView } from "@/components/preference/list";
import keybindings from "@/data/keybindings";

export default function Page() {
  return <UserPrefView array={keybindings} />;
}
