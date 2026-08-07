import { Check } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import {
  ListBoxItem as AriaListBoxItem,
  ListBoxItemProps as AriaListBoxItemProps,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import { elementFocusVisible, focusStyles } from "../../../lib/styling";
import { tv } from "../../../lib/variants";

const listBoxItemVariants = tv({
  base: [
    "rounded-md cursor-default transition-colors inline-flex gap-2 items-center justify-between bg-content text-content-fg",
    "truncate",
    "data-[hovered]:bg-neutral",
    focusStyles,
    elementFocusVisible,
  ],
  compoundVariants: [
    {
      class: "px-1.5 py-1",
      compact: true,
      size: "md",
    },
    {
      class: "px-1 py-1",
      compact: true,
      size: "sm",
    },
  ],
  defaultVariants: {
    size: "md",
  },
  variants: {
    compact: { true: "rounded-sm" },
    size: {
      md: "text-sm px-3 py-2",
      sm: "text-xs px-3 py-2",
    },
  },
});

type ListBoxItemProps = AriaListBoxItemProps &
  VariantProps<typeof listBoxItemVariants> & {
    children?: React.ReactNode;
    className?: string;
  };

export const ListBoxItem = ({
  children,
  className,
  compact,
  size,
  ...props
}: ListBoxItemProps) => {
  return (
    <AriaListBoxItem
      {...props}
      className={listBoxItemVariants({ className, compact, size })}
    >
      {({ isSelected }) => (
        <>
          <div className="truncate">{children}</div>
          <AnimatePresence>
            {isSelected && (
              <motion.div
                animate={{ scale: 1 }}
                exit={{ scale: 0 }}
                initial={{ scale: 0 }}
              >
                <Check
                  className="text-success transition-colors"
                  size={size === "md" ? 16 : 14}
                  strokeWidth={3}
                />
              </motion.div>
            )}
          </AnimatePresence>
        </>
      )}
    </AriaListBoxItem>
  );
};
