import { AnimatePresence, motion, MotionProps } from "motion/react";
import { AriaToggleButtonProps } from "react-aria";
import { ToggleButton as AriaToggleButton } from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import {
  availableVariants,
  elementFocusVisible,
  focusStyles,
} from "../../../lib/styling";
import { tv } from "../../../lib/variants";

const toggleButtonVariants = tv({
  base: [
    "relative flex justify-center items-center text-muted outline-none transition-colors",
    "data-[selected]:text-content-fg",
  ],
  compoundVariants: [
    {
      class: "px-3 py-2",
      size: "md",
      variant: "solid",
    },
    {
      class: "px-2 py-1",
      size: "sm",
      variant: "solid",
    },
    {
      class: elementFocusVisible,
      showFocus: true,
      variant: "solid",
    },
    {
      class: [
        "border-muted/30",
        "data-[hovered]:bg-neutral/50 data-[pressed]:bg-neutral/80 data-[selected]:bg-neutral",
      ],
      color: "neutral",
      variant: "solid",
    },
    {
      class: [
        "border-muted/30",
        "data-[hovered]:bg-error/10 data-[pressed]:bg-error/15 data-[selected]:bg-error/33 data-[selected]:border-error/50",
      ],
      color: "error",
      variant: "solid",
    },
  ],
  defaultVariants: {
    color: "neutral",
    showFocus: true,
    size: "md",
    variant: "solid",
  },
  variants: {
    color: availableVariants("neutral", "error"),
    showFocus: availableVariants("true"),
    size: { md: "text-md", sm: "text-xs" },
    variant: {
      ghost:
        "p-1 origin-center transform-gpu backface-hidden will-change-transform transition-transform data-[hovered]:scale-110 data-[pressed]:scale-105",
      solid: ["border-1 rounded-md", focusStyles],
    },
  },
});

type ToggleButtonProps = AriaToggleButtonProps &
  VariantProps<typeof toggleButtonVariants> & {
    children: React.ReactNode;
    className?: string;
    off?: React.ReactNode;
  };

export const ToggleButton = ({
  children,
  className,
  color,
  off,
  size,
  variant = "solid",
  ...props
}: ToggleButtonProps) => {
  const animationProps: MotionProps = {
    animate: { opacity: 1, scale: 1 },
    exit: { opacity: 0, scale: 0 },
    initial: { opacity: 0, scale: 0 },
  };

  return (
    <AriaToggleButton
      {...props}
      className={toggleButtonVariants({ className, color, size, variant })}
    >
      {({ isSelected }) => (
        <>
          {variant === "solid" && (isSelected ? children : (off ?? children))}

          {variant === "ghost" && (
            <>
              <div className="invisible">{children}</div>

              <AnimatePresence>
                {isSelected ? (
                  <motion.div
                    key="selected"
                    {...animationProps}
                    className="absolute inset-0 flex items-center justify-center text-content-fg"
                  >
                    {children}
                  </motion.div>
                ) : (
                  <motion.div
                    key="deselected"
                    {...animationProps}
                    className="absolute inset-0 flex items-center justify-center text-muted"
                  >
                    {off ?? children}
                  </motion.div>
                )}
              </AnimatePresence>
            </>
          )}
        </>
      )}
    </AriaToggleButton>
  );
};
