import { ReactNode } from "react";
import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";

const badgeVariants = tv({
  base: "flex flex-row items-center justify-center",
  defaultVariants: {
    color: "neutral",
    size: "md",
    variant: "outline",
  },
  variants: {
    color: {
      error: "border-error text-error",
      info: "border-info text-info",
      neutral: "border-muted text-muted",
      warning: "border-warning text-warning",
    },
    size: {
      md: "gap-1 rounded-lg px-2 py-1 text-sm",
      sm: "gap-1 rounded-md px-1.5 py-0.5 text-xs",
    },
    variant: {
      ghost: "",
      outline: "border-1",
    },
  },
});

type BadgeProps = VariantProps<typeof badgeVariants> & {
  children: ReactNode;
  className?: string;
};

export function Badge({
  children,
  className,
  color,
  size,
  variant,
}: BadgeProps) {
  return (
    <div className={badgeVariants({ className, color, size, variant })}>
      {children}
    </div>
  );
}
