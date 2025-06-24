import { cn } from "@/lib/utils";
import React, { forwardRef, useState } from "react";

type TerminalTabProps = React.HTMLAttributes<HTMLDivElement>;

const TerminalTab = forwardRef<HTMLDivElement, TerminalTabProps>(
  ({ children, ...props }, ref) => {
    return (
      <div ref={ref} {...props}>
        {children}
      </div>
    );
  },
);

interface TerminalProps {
  title?: string;
  tabNames?: string[];
  className?: string;
  children:
    | React.ReactElement<TerminalTabProps>
    | React.ReactElement<TerminalTabProps>[];
}

function Terminal({ title, tabNames, className, children }: TerminalProps) {
  const [activeTab, setActiveTab] = useState(0);
  const multipleTabs = tabNames?.length ? tabNames.length > 1 : false;
  return (
    <div
      className={cn(
        "place-self-center border rounded-lg border-zinc-800 w-full max-w-lg relative overflow-hidden select-none",
        className,
      )}
    >
      <div className="w-full h-auto rounded-lg flex flex-col bg-[#161A1D]">
        <div
          className={cn(
            "flex flex-row items-center gap-1.5 p-2 bg-zinc-700 rounded-t relative",
            title && "h-7",
          )}
        >
          <div className="w-2.5 h-2.5 rounded-full bg-red-500" />
          <div className="w-2.5 h-2.5 rounded-full bg-yellow-500" />
          <div className="w-2.5 h-2.5 rounded-full bg-green-500" />
          {title && (
            <div className="absolute left-1/2 top-1/2 transform -translate-x-1/2 -translate-y-1/2">
              <span className="text-xs text-zinc-200 font-mono cursor-default">
                {title}
              </span>
            </div>
          )}
        </div>
        {tabNames && (
          <div
            className="grid border-b-zinc-900 border-b-2 gap-0.5"
            style={{
              gridTemplateColumns: `repeat(${tabNames.length}, minmax(0, 1fr))`,
            }}
          >
            {tabNames.map((name, index) => (
              <div
                key={`${name}-${index}`}
                className={cn(
                  "text-zinc-400 text-center p-1 transition-colors font-mono cursor-default text-[0.675rem]",
                  multipleTabs &&
                    "hover:bg-zinc-800 hover:border-transparent cursor-pointer border-t border-zinc-950",
                  activeTab == index &&
                    "bg-zinc-700 hover:bg-zinc-700 border-transparent text-zinc-100",
                )}
                onClick={() => setActiveTab(index)}
              >
                {name}
              </div>
            ))}
          </div>
        )}

        <div className="p-2">
          {Array.isArray(children) ? children[activeTab] : children}
        </div>
      </div>
    </div>
  );
}
Terminal.Tab = TerminalTab;

export { Terminal };
