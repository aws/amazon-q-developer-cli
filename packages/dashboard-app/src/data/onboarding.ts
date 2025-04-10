import { InstallCheck } from "@/types/preferences";
import installChecks from "./install";
import { PRODUCT_NAME } from "@/lib/constants";

const onboardingSteps: InstallCheck[] = [
  {
    id: "welcome",
    title: `Welcome to ${PRODUCT_NAME}`,
    description: [""],
    action: "Continue",
  },
  ...installChecks,
  {
    id: "login",
    title: "Signed in with Builder ID",
    description: [
      "AI features won't work if you're no longer signed into Builder ID.",
    ],
    action: "Sign in",
  },
];

export default onboardingSteps;
