import { AwsLogo } from "@/components/svg/icons";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Input } from "@/components/ui/input";
import { useLocalStateZodDefault } from "@/hooks/store/useState";
import { z } from "zod";
import { AMZN_START_URL } from "@/lib/constants";
import { useEffect, useState, useCallback } from "react";
import { Profile } from "@aws/amazon-q-developer-cli-api-bindings";
import { State } from "@aws/amazon-q-developer-cli-api-bindings";

function BuilderIdTab({
  handleLogin,
  toggleTab,
  signInText,
}: {
  handleLogin: () => void;
  toggleTab: () => void;
  signInText: string;
}) {
  return (
    <div className="flex flex-col items-center gap-1">
      <Button
        variant="glass"
        onClick={() => handleLogin()}
        className="flex gap-4 pl-2 self-center"
      >
        <AwsLogo />
        {signInText}
      </Button>
      <Button
        className="h-auto p-1 px-2 hover:bg-white/20 hover:text-white"
        variant={"ghost"}
        onClick={toggleTab}
      >
        <span className="text-xs">Use with Pro license</span>
      </Button>
    </div>
  );
}

function IamInput({
  title,
  description,
  input,
}: {
  title: string;
  description: string;
  input: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <p className="font-bold leading-tight">{title}</p>
      <p className="text-xs mb-1 text-zinc-100">{description}</p>
      {input}
    </div>
  );
}

function IamTab({
  handleLogin,
  toggleTab,
  signInText,
}: {
  handleLogin: (startUrl: string, region: string) => void;
  toggleTab: () => void;
  signInText: string;
}) {
  const midway = window?.fig?.constants?.midway;

  // TODO: this should be fetched from https://idetoolkits.amazonwebservices.com/endpoints.json
  const REGIONS = {
    "af-south-1": {
      description: "Africa (Cape Town)",
    },
    "ap-east-1": {
      description: "Asia Pacific (Hong Kong)",
    },
    "ap-northeast-1": {
      description: "Asia Pacific (Tokyo)",
    },
    "ap-northeast-2": {
      description: "Asia Pacific (Seoul)",
    },
    "ap-northeast-3": {
      description: "Asia Pacific (Osaka)",
    },
    "ap-south-1": {
      description: "Asia Pacific (Mumbai)",
    },
    "ap-south-2": {
      description: "Asia Pacific (Hyderabad)",
    },
    "ap-southeast-1": {
      description: "Asia Pacific (Singapore)",
    },
    "ap-southeast-2": {
      description: "Asia Pacific (Sydney)",
    },
    "ap-southeast-3": {
      description: "Asia Pacific (Jakarta)",
    },
    "ap-southeast-4": {
      description: "Asia Pacific (Melbourne)",
    },
    "ca-central-1": {
      description: "Canada (Central)",
    },
    "ca-west-1": {
      description: "Canada West (Calgary)",
    },
    "eu-central-1": {
      description: "Europe (Frankfurt)",
    },
    "eu-central-2": {
      description: "Europe (Zurich)",
    },
    "eu-north-1": {
      description: "Europe (Stockholm)",
    },
    "eu-south-1": {
      description: "Europe (Milan)",
    },
    "eu-south-2": {
      description: "Europe (Spain)",
    },
    "eu-west-1": {
      description: "Europe (Ireland)",
    },
    "eu-west-2": {
      description: "Europe (London)",
    },
    "eu-west-3": {
      description: "Europe (Paris)",
    },
    "il-central-1": {
      description: "Israel (Tel Aviv)",
    },
    "me-central-1": {
      description: "Middle East (UAE)",
    },
    "me-south-1": {
      description: "Middle East (Bahrain)",
    },
    "sa-east-1": {
      description: "South America (Sao Paulo)",
    },
    "us-east-1": {
      description: "US East (N. Virginia)",
    },
    "us-east-2": {
      description: "US East (Ohio)",
    },
    "us-west-1": {
      description: "US West (N. California)",
    },
    "us-west-2": {
      description: "US West (Oregon)",
    },
  } as const;

  const DEFAULT_SSO_REGION = "us-east-1";

  const [startUrl, setStartUrl] = useLocalStateZodDefault(
    "auth.idc.start-url",
    z.string(),
    midway ? AMZN_START_URL : "",
  );
  const [region, setRegion] = useLocalStateZodDefault(
    "auth.idc.region",
    z.string(),
    DEFAULT_SSO_REGION,
  );

  return (
    <div className="border rounded p-4 flex flex-col bg-black/20 gap-4">
      <div>
        <p className="font-bold text-lg">
          Sign in with AWS IAM Identity Center
        </p>
      </div>
      <IamInput
        title="Start URL"
        description="Provided by your admin or help desk"
        input={
          <Input
            value={startUrl}
            onChange={(e) => setStartUrl(e.target.value)}
            className="text-black dark:text-white"
            type="url"
          />
        }
      />
      <IamInput
        title="Region"
        description="AWS Region that hosts identity directory"
        input={
          <Select onValueChange={(value) => setRegion(value)} value={region}>
            <SelectTrigger className="w-full text-black dark:text-white">
              <SelectValue placeholder="Theme" />
            </SelectTrigger>
            <SelectContent className="h-96">
              {Object.entries(REGIONS).map(([key, value]) => (
                <SelectItem key={key} value={key}>
                  <span className="font-mono mr-2">{key}</span>
                  <span className="text-xs opacity-70">
                    {value.description}
                  </span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        }
      />
      <div className="flex flex-col items-center gap-1 mt-2">
        <Button
          variant="glass"
          onClick={() => handleLogin(startUrl, region)}
          className="flex gap-4 pl-2 self-center"
        >
          <AwsLogo />
          {signInText}
        </Button>
        <Button
          className="h-auto p-1 px-2 hover:bg-white/20 hover:text-white"
          variant={"ghost"}
          onClick={toggleTab}
        >
          <span className="text-xs">Use for Free with Builder ID</span>
        </Button>
      </div>
    </div>
  );
}

type ProfileData = { profileName: string; arn: string };

export function ProfileTab({
  next,
  back,
}: {
  next: () => void;
  back: () => void;
}) {
  const [selectedProfile, setSelectedProfile] = useState<ProfileData | null>(
    null,
  );
  const [profiles, setProfiles] = useState<Array<ProfileData>>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Centralized function to set profile to avoid duplication
  const handleSetProfile = useCallback(
    (profile: ProfileData | null) => {
      if (!profile || isSubmitting) return;

      setIsSubmitting(true);

      // Set the profile
      Profile.setProfile(profile.profileName, profile.arn)
        .then(() => {
          next();
        })
        .catch((_) => {
          setError("Failed to set profile. Please try again.");
          setIsSubmitting(false);
        });
    },
    [isSubmitting, next],
  );

  useEffect(() => {
    // Fetch available profiles
    Profile.listAvailableProfiles()
      .then(async (res) => {
        if (res.profiles && res.profiles.length > 0) {
          setProfiles(
            res.profiles.map((p) => ({
              profileName: p.profileName,
              arn: p.arn,
            })),
          );
        } else {
          setProfiles([]);
        }
      })
      .catch((_) => {
        setError("Failed to fetch available profiles");
      })
      .finally(async () => {
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    // Try to get current profile if available and swallow the error otherwise
    State.get("api.codewhisperer.profile")
      .then((profile) => {
        if (profile) {
          setSelectedProfile(profile);
        }
      })
      .catch(() => {});
  }, []);

  // If there's only one profile, automatically select it and continue
  useEffect(() => {
    if (!loading && profiles.length === 1) {
      const profile = profiles[0];
      handleSetProfile(profile);
    }
  }, [loading, profiles, handleSetProfile]);

  return (
    <div className="flex flex-col items-center gap-8 gradient-q-secondary-light -m-10 p-10 rounded-lg text-white">
      <h2 className="text-xl text-white font-semibold select-none leading-none font-ember tracking-tight">
        Select a profile
      </h2>

      {error && (
        <div className="flex flex-col items-center gap-2 w-full bg-red-200 border border-red-600 rounded py-2 px-2">
          <p className="text-black dark:text-white font-semibold text-center">
            Error
          </p>
          <p className="text-black dark:text-white text-center">{error}</p>
        </div>
      )}

      <div className="flex flex-col items-center gap-4 text-white text-sm w-full max-w-md">
        {loading ? (
          <div className="flex flex-col items-center justify-center h-20 w-full">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-white"></div>
            <p className="mt-4">Loading profiles...</p>
          </div>
        ) : (
          <>
            <div className="w-full max-h-60 overflow-y-auto pr-2">
              {profiles.map((profileItem) => (
                <div
                  key={profileItem.arn}
                  className={`p-3 mb-2 rounded-md cursor-pointer border ${
                    selectedProfile && selectedProfile.arn === profileItem.arn
                      ? "bg-white/20 border-white"
                      : "bg-white/5 border-transparent hover:bg-white/10"
                  }`}
                  onClick={() => {
                    if (!isSubmitting) {
                      setSelectedProfile(profileItem);
                    }
                  }}
                >
                  <div className="font-medium">{profileItem.profileName}</div>
                  <div className="text-xs text-white/70 truncate">
                    {profileItem.arn}
                  </div>
                </div>
              ))}
            </div>

            <div className="flex justify-between w-full mt-4">
              <Button onClick={back}>Back</Button>
              <Button
                onClick={() => handleSetProfile(selectedProfile)}
                disabled={!selectedProfile || isSubmitting}
              >
                {isSubmitting ? "Setting profile..." : "Continue"}
              </Button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export default function Tab({
  tab,
  handleLogin,
  toggleTab,
  signInText,
}: {
  tab: "builderId" | "iam" | "profile";
  handleLogin: () => void;
  toggleTab: () => void;
  signInText: string;
}) {
  switch (tab) {
    case "builderId":
      return (
        <BuilderIdTab
          handleLogin={handleLogin}
          toggleTab={toggleTab}
          signInText={signInText}
        />
      );
    case "iam":
      return (
        <IamTab
          handleLogin={handleLogin}
          toggleTab={toggleTab}
          signInText={signInText}
        />
      );
  }
}
