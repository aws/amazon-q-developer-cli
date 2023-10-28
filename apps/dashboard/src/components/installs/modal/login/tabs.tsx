import { AwsLogo } from "@/components/svg/icons";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../../ui/select"
import { Input } from "../../../ui/input";
import { useState } from "react";

function BuilderIdTab({ handleLogin, toggleTab }: { handleLogin: () => void, toggleTab: () => void }) {
  return (
    <div className="flex flex-col items-center gap-1">
    <Button
        variant="glass"
        onClick={() => handleLogin()}
        className="flex gap-4 pl-2 self-center"
      >
        <AwsLogo />
        Sign in
      </Button>
      <Button className="h-auto p-1 px-2 hover:bg-white/20 hover:text-white" variant={'ghost'} onClick={toggleTab}>
        <span className="text-xs">Pro account? Switch to IAM Identity Center</span>
      </Button>
    </div>
  );
}

function IamTab({ handleLogin, toggleTab }: { handleLogin: (startUrl: string, region: string) => void, toggleTab: () => void }) {
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

  const [startUrl, setStartUrl] = useState("");
  const [region, setRegion] = useState(DEFAULT_SSO_REGION);

  return (
    <div className="border rounded p-4 flex flex-col bg-black/20 gap-4">
      <div>
        <p className="font-bold text-lg">IAM Identity Center</p>
        <p>Successor to AWS Single Sign-on</p>
      </div>
      <div className="flex flex-col gap-1">
        <p className="font-bold">Start URL</p>
        <p>URL for your organization, provided by an admin or help desk.</p>
        <Input
          value={startUrl}
          onChange={(e) => setStartUrl(e.target.value)}
          className="text-black"
          type="url"
        />
      </div>
      <div className="flex flex-col gap-1">
        <p className="font-bold">Region</p>
        <p>AWS Region that hosts Identity directory</p>
        <Select onValueChange={(value) => setRegion(value)} value={region}>
          <SelectTrigger className="w-full text-black">
            <SelectValue placeholder="Theme" />
          </SelectTrigger>
          <SelectContent className="h-96">
            {Object.entries(REGIONS).map(([key, value]) => (
              <SelectItem key={key} value={key}>
                <span className="font-mono mr-2">{key}</span>
                <span className="text-xs text-neutral-600">
                  {value.description}
                </span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="flex flex-col items-center gap-1">
    <Button
        variant="glass"
        onClick={() => handleLogin(startUrl, region)}
        className="flex gap-4 pl-2 self-center"
      >
        <AwsLogo />
        Sign in
      </Button>
      <Button className="h-auto p-1 px-2 hover:bg-white/20 hover:text-white" variant={'ghost'} onClick={toggleTab}>
        <span className="text-xs">Personal account? Use Builder ID</span>
      </Button>
    </div>
    </div>
  );
}

export default function Tab({
  tab,
  handleLogin,
  toggleTab
}: {
  tab: "builderId" | "iam";
  handleLogin: () => void;
  toggleTab: () => void
}) {
  switch (tab) {
    case "builderId":
      return <BuilderIdTab handleLogin={handleLogin} toggleTab={toggleTab} />;
    case "iam":
      return <IamTab handleLogin={handleLogin} toggleTab={toggleTab} />;
  }
}