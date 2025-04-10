import * as React from "react";
import * as SwitchPrimitives from "@radix-ui/react-switch";

import { cn } from "@/lib/utils";

import { cva, type VariantProps } from "class-variance-authority";
const variants = cva(
  "peer inline-flex h-[24px] w-[44px] shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50",
  {
    variants: {
      variant: {
        default:
          "data-[state=checked]:bg-dusk-500 data-[state=unchecked]:bg-dusk-200 dark:data-[state=checked]:bg-dusk-500 dark:data-[state=unchecked]:bg-zinc-950",
        monotone:
          "data-[state=checked]:bg-black data-[state=unchecked]:bg-input",
        inverted:
          "data-[state=checked]:bg-transparent data-[state=unchecked]:bg-white/30 border-white h-[28px] w-[48px]",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

const thumbVariants = cva(
  "pointer-events-none block h-5 w-5 rounded-full bg-background dark:bg-zinc-300 dark:data-[state=checked]:bg-dusk-100 shadow-lg ring-0 transition-transform data-[state=checked]:translate-x-5 data-[state=unchecked]:translate-x-0",
  {
    variants: {
      variant: {
        default: "",
        monotone: "",
        inverted:
          "data-[state=checked]:translate-x-[22px] data-[state=unchecked]:translate-x-[2px]",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

interface SwitchProps
  extends React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>,
    VariantProps<typeof variants> {}

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  SwitchProps
>(({ className, variant, ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cn(variants({ variant, className }))}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb
      className={cn(thumbVariants({ variant, className }))}
    />
  </SwitchPrimitives.Root>
));
Switch.displayName = SwitchPrimitives.Root.displayName;

export { Switch };
