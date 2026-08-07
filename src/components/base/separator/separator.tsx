import { SeparatorProps as AriaSeparatorProps, useSeparator } from "react-aria";
import { VariantProps } from "tailwind-variants";

import { availableVariants } from "../../../lib/styling";
import { tv } from "../../../lib/variants";

const separatorVariants = tv({
  base: "relative bg-content-fg/20 rounded-xs m-auto",
  compoundVariants: [
    {
      class: "my-1",
      orientation: "horizontal",
      spacing: "sm",
    },
    {
      class: "my-2",
      orientation: "horizontal",
      spacing: "md",
    },
    { class: "mx-2", orientation: "vertical", spacing: "sm" },
    { class: "mx-4", orientation: "vertical", spacing: "md" },
  ],
  defaultVariants: {
    orientation: "horizontal",
    spacing: "sm",
  },
  variants: {
    orientation: {
      horizontal: "w-full h-[1px]",
      vertical: "h-full w-[1px]",
    },
    spacing: availableVariants("sm", "md"),
  },
});

type SeparatorProps = AriaSeparatorProps &
  VariantProps<typeof separatorVariants> & {
    children?: React.ReactNode;
    className?: string;
  };

export const Separator = ({
  children,
  className,
  orientation,
  spacing,
  ...rest
}: SeparatorProps) => {
  const { separatorProps } = useSeparator(rest);

  return (
    <div
      {...separatorProps}
      className={separatorVariants({ className, orientation, spacing })}
    >
      {children != null && (
        <div className="absolute inset-0 flex items-center justify-center">
          {children}
        </div>
      )}
    </div>
  );
};
