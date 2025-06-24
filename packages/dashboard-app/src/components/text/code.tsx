import { cn } from "@/lib/utils";

export function Code({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <code
      className={cn(
        "text-[0.9em] px-1 bg-zinc-50 border border-zinc-200 rounded-sm text-zinc-600 dark:border-zinc-700 dark:text-zinc-300 dark:bg-zinc-900 whitespace-nowrap",
        className,
      )}
    >
      {children}
    </code>
  );
}
