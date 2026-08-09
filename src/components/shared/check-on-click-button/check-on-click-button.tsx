// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Check } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { ComponentProps, useState } from "react";
import { PressEvent } from "react-aria";
import { VariantProps } from "tailwind-variants";

import { availableVariants, cn } from "../../../lib/styling";
import { tv } from "../../../lib/variants";
import { Button } from "../../base/button/button";

const checkOnClickButtonVariants = tv({
  base: "absolute inset-0 flex items-center justify-center transition-all rounded-md backdrop-blur-none",
  compoundVariants: [
    {
      blur: "md",
      class: "backdrop-blur-md",
      isClicked: true,
    },
    {
      blur: "xs",
      class: "backdrop-blur-xs",
      isClicked: true,
    },
  ],
  variants: {
    blur: availableVariants("md", "xs"),
    isClicked: {
      true: "bg-content/50",
    },
  },
});

type CheckOnClickButtonProps = ComponentProps<typeof Button> &
  VariantProps<typeof checkOnClickButtonVariants>;

/** Shows a check after pressing, has no concept of success/fail. */
export const CheckOnClickButton = ({
  blur = "md",
  children,
  className,
  onPress,
  ...props
}: CheckOnClickButtonProps) => {
  const [isClicked, setIsClicked] = useState(false);

  const handlePress = (e: PressEvent) => {
    onPress?.(e);

    setIsClicked(true);
    setTimeout(() => {
      setIsClicked(false);
    }, 2000);
  };
  return (
    <Button
      {...props}
      className={cn(className, "relative")}
      // The caller's own disabled state has to survive, or a button that is
      // meant to be unavailable stays pressable between checks.
      isDisabled={Boolean(props.isDisabled) || isClicked}
      onPress={handlePress}
    >
      {children}

      <div className={checkOnClickButtonVariants({ blur, isClicked })}>
        <AnimatePresence>
          {isClicked && (
            <motion.span
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -5 }}
              initial={{ opacity: 0, y: 5 }}
              transition={{ duration: 0.2 }}
            >
              <Check className="text-success" strokeWidth={3} />
            </motion.span>
          )}
        </AnimatePresence>
      </div>
    </Button>
  );
};
