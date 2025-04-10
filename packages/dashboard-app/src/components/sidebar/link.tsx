import { cn } from "@/lib/utils";
import { NavLink } from "react-router-dom";

const defaultStyles =
  "p-2 h-10 hover:bg-dusk-100 dark:hover:bg-zinc-900 text-zinc-500 dark:text-zinc-400 dark:hover:text-white/80 transition-colors w-full rounded-lg flex flex-row items-center justify-between font-light [&>span]:bg-zinc-500 [&>span]:text-white hover:text-dusk-600 [&:hover>span]:bg-dusk-600 border border-transparent";
const activeStyles = cn(
  defaultStyles,
  "bg-dusk-600 hover:bg-dusk-600 text-white dark:text-dusk-100 [&>span]:bg-white [&>span]:text-dusk-600 [&:hover>span]:text-dusk-600 [&:hover>span]:bg-white hover:text-white",
);
const errorStyle = cn(
  defaultStyles,
  "hover:bg-red-100 dark:hover:bg-red-900 dark:hover:text-red-100 text-red-600 dark:text-white [&>span]:bg-red-500 [&>span]:text-white [&:hover>span]:text-white [&:hover>span]:bg-red-500 hover:text-red-600 border-red-400",
);
const activeErrorStyle = cn(
  defaultStyles,
  "bg-red-600 hover:bg-red-700 hover:text-white text-white dark:text-white [&>span]:bg-white [&>span]:text-red-500 [&:hover>span]:text-red-500 [&:hover>span]:bg-white hover:text-white border-red-600 hover:border-red-700",
);

export default function SidebarLink({
  icon,
  name,
  path = "/",
  count,
  error,
}: {
  icon: React.ReactNode;
  name: string;
  path?: string;
  count?: number;
  error?: boolean;
}) {
  return (
    <NavLink
      to={path}
      className={({ isActive }) =>
        error
          ? isActive
            ? activeErrorStyle
            : errorStyle
          : isActive
            ? activeStyles
            : defaultStyles
      }
    >
      <div className="flex flex-row items-center gap-2 fill-current whitespace-nowrap">
        {icon}
        <span className="text-base select-none">{name}</span>
      </div>
      {error && (
        <span className="flex items-center justify-center leading-none p-1 px-2 rounded-full font-bold text-xs select-none">
          !
        </span>
      )}
      {count != undefined && count > 0 && !error && (
        <span className="flex items-center justify-center leading-none p-1 px-2 rounded-full font-medium text-sm select-none">
          {count > 9 ? "9+" : count}
        </span>
      )}
    </NavLink>
  );
}
