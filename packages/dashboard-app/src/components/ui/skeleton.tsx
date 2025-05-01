import { cn } from "@/lib/utils";

function Skeleton({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "animate-pulse rounded-md bg-background dark:border-zinc-700 dark:bg-zinc-900",
        className,
      )}
      {...props}
    />
  );
}

export { Skeleton };
