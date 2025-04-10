import { Button } from "@/components/ui/button";

type Integration = {
  id: string;
  title: string;
  background?: {
    color: string;
    image: string;
  };
  description?: string;
};

export default function IntegrationCard({
  integration,
  enabled,
  toggleSwitch,
}: {
  integration: Integration;
  enabled: boolean;
  toggleSwitch: () => void;
}) {
  const integrationId = integration.id.split(".")[1];

  return (
    <div className="flex flex-col relative overflow-hidden rounded-lg p-4 pt-6 gap-4 max-w-xs border border-black/10 dark:border-white/10 col-span-1">
      <div
        style={{
          backgroundImage: `url(${`/images/integrations/bg/${integrationId}.svg`})`,
        }}
        className="absolute left-0 right-0 bottom-1/2 w-full h-full bg-no-repeat bg-cover"
      />
      <div className="flex flex-col relative z-10 items-center text-center">
        <h3 className="text-white font-ember text-xl font-bold">
          {integration.title}
        </h3>
        <img
          className="h-40 w-40"
          src={`/images/integrations/icons/${integrationId}.png`}
        />
      </div>
      <p className="text-black/50 dark:text-white/50 self-center text-center flex-auto">
        {integration.description}
      </p>
      {enabled === true ? (
        <Button
          onClick={toggleSwitch}
          variant={"outline"}
          className="group text-black/50 hover:text-black"
        >
          <span className="inline-block group-hover:hidden">Enabled</span>
          <span className="hidden group-hover:inline-block">Disable</span>
        </Button>
      ) : (
        <Button onClick={toggleSwitch} variant={"default"}>
          Enable
        </Button>
      )}
    </div>
  );
}
