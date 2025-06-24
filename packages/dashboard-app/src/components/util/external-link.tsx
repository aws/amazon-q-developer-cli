import { cn } from "@/lib/utils";
import { Native } from "@aws/amazon-q-developer-cli-api-bindings";

export default function ExternalLink({
  href,
  onClick,
  className,
  ...props
}: { href: string } & React.HTMLAttributes<HTMLAnchorElement>) {
  return (
    <a
      target="_blank"
      rel="noopener noreferrer"
      className={cn("cursor-pointer", className)}
      href={href}
      onClick={(e) => {
        if (window.fig) {
          e.preventDefault();
          Native.open(href).catch(console.error);
        }
        onClick?.(e);
      }}
      {...props}
    />
  );
}
