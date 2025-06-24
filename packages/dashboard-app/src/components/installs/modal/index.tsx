import { InstallCheck } from "@/types/preferences";
import { Fig, Internal } from "@aws/amazon-q-developer-cli-api-bindings";
import { useEffect, useState } from "react";
import { Button } from "../../ui/button";
import Lockup from "../../svg/logo";
import onboarding from "@/data/onboarding";
import { useStatusCheck } from "@/hooks/store/useStatusCheck";
import LoginModal from "./login";
import InstallModal from "./install";
import { useLocalState, useRefreshLocalState } from "@/hooks/store/useState";
import migrate_dark from "@assets/images/fig-migration/dark.png?url";
import { usePlatformInfo } from "@/hooks/store/usePlatformInfo";
import { matchesPlatformRestrictions } from "@/lib/platform";

export default function OnboardingModal() {
  const [step, setStep] = useState(0);
  const check = onboarding[step] as InstallCheck;
  const [migrationStarted] = useLocalState("desktop.migratedFromFig");
  const [migrationEnded, setMigrationEnded] = useLocalState(
    "desktop.migratedFromFig.UiComplete",
  );
  const [dotfilesCheck, refreshDotfiles] = useStatusCheck("dotfiles");
  const [accessibilityCheck, refreshAccessibility] =
    useStatusCheck("accessibility");
  const [_desktopEntryCheck, refreshDesktopEntry] =
    useStatusCheck("desktopEntry");
  const [_gnomeExtensionCheck, refreshGnomeExtension] =
    useStatusCheck("gnomeExtension");
  const refreshLocalState = useRefreshLocalState();
  const platformInfo = usePlatformInfo();

  const [_dotfiles, setDotfiles] = useState(dotfilesCheck);
  const [_accessibility, setAccessibility] = useState(accessibilityCheck);

  const isMigrating =
    Boolean(migrationStarted) === true && Boolean(migrationEnded) === false;

  useEffect(() => {
    refreshAccessibility();
    refreshDotfiles();
    refreshDesktopEntry();
    refreshGnomeExtension();
  }, [
    refreshAccessibility,
    refreshDotfiles,
    refreshDesktopEntry,
    refreshGnomeExtension,
  ]);

  function nextStep() {
    if (migrationStarted && !migrationEnded) {
      setMigrationEnded(true);
    }

    setStep(step + 1);
  }

  function finish() {
    refreshAccessibility();
    refreshDotfiles();
    refreshDesktopEntry();
    refreshGnomeExtension();
    Internal.sendOnboardingRequest({
      action: Fig.OnboardingAction.FINISH_ONBOARDING,
    })
      .then(() => {
        refreshLocalState().catch((err) => console.error(err));
      })
      .catch((err) => console.error(err));
  }

  function skipInstall() {
    if (!check.id) return;

    if (check.id === "dotfiles") {
      setDotfiles(true);
      setStep(step + 1);
    }

    if (check.id === "accessibility") {
      setAccessibility(true);
      setStep(step + 1);
    }

    if (check.id === "desktopEntry" || check.id === "gnomeExtension") {
      setStep(step + 1);
    }
  }

  if (
    platformInfo &&
    !matchesPlatformRestrictions(platformInfo, check.platformRestrictions)
  ) {
    setStep(step + 1);
  }

  if (check.id === "welcome" && isMigrating) {
    return <WelcomeModal figMigration next={nextStep} />;
  }

  if (check.id === "welcome") {
    return <WelcomeModal next={nextStep} />;
  }

  if (
    ["dotfiles", "accessibility", "gnomeExtension", "desktopEntry"].includes(
      check.id,
    )
  ) {
    return <InstallModal check={check} skip={skipInstall} next={nextStep} />;
  }

  if (check.id === "login") {
    return <LoginModal next={finish} />;
  }

  return null;
}

export function WelcomeModal({
  next,
  figMigration,
}: {
  next: () => void;
  figMigration?: boolean;
}) {
  return (
    <div className="flex flex-col items-center gap-8 gradient-q-secondary-light -m-10 p-4 pt-10 rounded-lg text-white">
      <div className="flex flex-col items-center gap-8">
        {figMigration ? (
          <img src={migrate_dark} className="w-40" />
        ) : (
          <Lockup />
        )}
        <div className="flex flex-col gap-2 items-center text-center">
          <h2 className="text-2xl text-white font-semibold select-none leading-none font-ember tracking-tight">
            {figMigration ? "Almost done upgrading!" : "Welcome!"}
          </h2>
          <p className="text-sm">Let's get you set up...</p>
        </div>
      </div>
      <div className="flex flex-col items-center gap-2 text-white text-sm font-bold">
        <Button variant="glass" onClick={() => next()} className="flex gap-4">
          Get started
        </Button>
      </div>
    </div>
  );
}
