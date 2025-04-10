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

export default function Tab({
  tab,
  handleLogin,
  toggleTab,
  signInText,
}: {
  tab: "builderId" | "iam";
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
