import { State, Process } from "@aws/amazon-q-developer-cli-api-bindings";
import { useEffect, useState } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Code } from "@/components/text/code";

export default function Account() {
  const [profile, setProfile] = useState<string | undefined>(undefined);
  const [profiles, setProfiles] = useState<string[] | undefined>(undefined);

  useEffect(() => {
    Process.run({
      executable: "/opt/homebrew/bin/aws",
      args: ["configure", "list-profiles"],
    })
      .then(async (res) => {
        if (res.exitCode === 0) {
          setProfiles(
            res.stdout
              .trim()
              .split("\n")
              .map((p) => p.trim()),
          );
        } else {
          console.error(res.stderr);
        }
      })
      .catch((err) => {
        console.error(err);
      });
  }, []);

  useEffect(() => {
    State.get("aws.profile").then((profile) => {
      if (typeof profile === "string") {
        setProfile(profile);
      }
    });
  }, []);

  const onProfileChange = (profile: string | undefined) => {
    setProfile(profile);
    if (profile) {
      State.set("aws.profile", profile);
    } else {
      State.remove("aws.profile");
    }
  };

  return (
    <div className="flex flex-col gap-1">
      <h2 className="text-lg font-medium">
        Note only AWS credentials are currently supported
      </h2>
      {(!profiles || profiles.length > 0) && (
        <div className="flex flex-col gap-1">
          <h2 className="text-lg font-medium">Select Profile</h2>
          <Select
            value={profile}
            onValueChange={(profile) => {
              onProfileChange(profile);
            }}
            disabled={!profiles}
          >
            <SelectTrigger className="w-60">
              <SelectValue placeholder="No Profile Selected" />
            </SelectTrigger>
            <SelectContent>
              {profiles &&
                profiles.map((p) => (
                  <SelectItem key={p} value={p}>
                    {p}
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {profiles?.length === 0 && (
        <>
          <h2 className="text-lg font-medium">No Profile Found</h2>
          <ol className="list-decimal pl-6">
            <li>
              <p>
                Install and configure Builder Toolbox if you have not done so
                already.
              </p>
            </li>
            <li>
              <p>
                Run <Code>mwinit</Code> on your laptop
              </p>
            </li>
            <li>
              <p>
                Install the ADA CLI: <Code>toolbox install ada</Code>
              </p>
            </li>
            <li className="space-y-2">
              <p>Add an account</p>
              <p>
                Update your new profile with credentials (replacing the profile
                name, account ID, and role name with your information):
              </p>

              <p>For Isengard accounts:</p>
              <div className="pl-4">
                <Code className="block">
                  ada profile add --profile profile-name --provider isengard
                  --account account_id --role role_name
                </Code>
              </div>
              <p>For Conduit accounts:</p>
              <div className="pl-4">
                <Code className="block">
                  ada profile add --profile profile-name --provider conduit
                  --account account_id --role role_name
                </Code>
              </div>
            </li>
          </ol>
        </>
      )}
    </div>
  );
}
