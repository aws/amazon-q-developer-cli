import { UserPrefView } from "@/components/preference/list";
import { Button } from "@/components/ui/button";
import settings from "@/data/preferences";
import { useAuth } from "@/hooks/store/useAuth";
import { Native, User } from "@aws/amazon-q-developer-cli-api-bindings";
import { State, Profile } from "@aws/amazon-q-developer-cli-api-bindings";
import { useEffect, useState } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";

type Profile = { profileName: string; arn: string };

export default function Page() {
  const auth = useAuth();
  const [profile, setProfile] = useState<Profile | undefined>(undefined);
  const [profiles, setProfiles] = useState<Profile[] | undefined>(undefined);

  useEffect(() => {
    Profile.listAvailableProfiles()
      .then(async (res) => {
        setProfiles(
          res.profiles.map((p) => ({
            profileName: p.profileName,
            arn: p.arn,
          })),
        );
      })
      .catch((err) => {
        console.error(err);
      });
  }, []);

  useEffect(() => {
    State.get("api.codewhisperer.profile").then((profile) => {
      if (typeof profile === "object") {
        setProfile(profile);
      }
    });
  }, []);

  const onProfileChange = (profile: Profile | undefined) => {
    setProfile(profile);
    if (profile) {
      State.set("api.codewhisperer.profile", profile);
    }
  };

  let authKind;
  switch (auth.authKind) {
    case "BuilderId":
      authKind = "Builder ID";
      break;
    case "IamIdentityCenter":
      authKind = "AWS IAM Identity Center";
      break;
  }

  function logout() {
    User.logout().then(() => {
      window.location.pathname = "/";
      window.location.reload();
    });
  }

  return (
    <>
      <UserPrefView array={settings} />
      <section className={`flex flex-col py-4`}>
        <h2
          id={`subhead-account`}
          className="font-bold text-medium text-zinc-400 leading-none mt-2"
        >
          Account
        </h2>
        <div className={`flex p-4 pl-0 gap-4`}>
          <div className="flex flex-col gap-1">
            <h3 className="font-medium leading-none">Account type</h3>
            <p className="font-light leading-tight text-sm">
              Users can log in with either AWS Builder ID or AWS IAM Identity
              Center
            </p>
            <p className="font-light leading-tight text-sm text-black/50 dark:text-white/50">
              {auth.authed
                ? authKind
                  ? `Logged in with ${authKind}`
                  : "Logged in"
                : "Not logged in"}
            </p>

            {auth.authed && auth.authKind === "IamIdentityCenter" && (
              <div className="py-4 flex flex-col gap-1">
                <h3 className="font-medium leading-none">Active Profile</h3>
                <p className="font-light leading-tight text-sm">
                  SSO users with multiple profiles can select them here
                </p>
                {profiles ? (
                  <Select
                    value={profile?.arn}
                    onValueChange={(profile) => {
                      onProfileChange(profiles?.find((p) => p.arn === profile));
                    }}
                    disabled={!profiles}
                  >
                    <SelectTrigger className="w-60">
                      <SelectValue placeholder="No Profile Selected" />
                    </SelectTrigger>
                    <SelectContent>
                      {profiles &&
                        profiles.map((p) => (
                          <SelectItem key={p.arn} value={p.arn}>
                            {p.profileName}
                          </SelectItem>
                        ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <Skeleton className="w-60 h-10" />
                )}
              </div>
            )}
            <div className="pt-2">
              <Button
                variant="outline"
                onClick={() => logout()}
                disabled={!auth.authed}
              >
                Log out
              </Button>
            </div>
          </div>
        </div>
      </section>
      <section className={`py-4 gap-4`}>
        <h2
          id={`subhead-licenses`}
          className="font-bold text-medium text-zinc-400 leading-none mt-2"
        >
          Licenses
        </h2>
        <Button
          variant="link"
          className="px-0 text-blue-500 hover:underline decoration-1 underline-offset-1 hover:text-blue-800 hover:underline-offset-4 transition-all duration-100 text-sm"
          onClick={() => {
            Native.open(
              "file:///Applications/Amazon Q.app/Contents/Resources/dashboard/license/NOTICE.txt",
            );
          }}
        >
          View licenses
        </Button>
      </section>
    </>
  );
}
