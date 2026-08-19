// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Check } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { ComponentProps, useEffect, useRef, useState } from "react";
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

type CheckOnClickButtonProps = Omit<ComponentProps<typeof Button>, "onPress"> &
  VariantProps<typeof checkOnClickButtonVariants> & {
    // Typed as `unknown` rather than `Promise<unknown> | void` so a plain
    // void handler still fits; a returned promise switches the button to its
    // awaited mode.
    onPress?: (e: PressEvent) => unknown;
  };

/**
 * Shows a check after pressing, in one of two modes depending on what the
 * `onPress` handler returns.
 *
 * - Returns `void`: optimistic. The check appears immediately on press and
 *   holds for two seconds; success and failure look identical.
 * - Returns a `Promise`: awaited. The button pulses while the promise is in
 *   flight and only shows the check once it resolves. A rejection returns the
 *   button to idle with no check — surfacing the error is the caller's job.
 *
 * The button is non-interactive while pending or checked, but is never marked
 * `isDisabled` for that: the disabled styling desaturates the check itself.
 */
export const CheckOnClickButton = ({
  blur = "md",
  children,
  className,
  onPress,
  ...props
}: CheckOnClickButtonProps) => {
  const [status, setStatus] = useState<"checked" | "idle" | "pending">("idle");
  const isClicked = status === "checked";

  // A press token discards the tail of any press that has been superseded, and
  // the mounted flag keeps a late promise from setting state on a dead tree.
  const pressTokenRef = useRef(0);
  const isMountedRef = useRef(false);
  // Set inside the effect, not at declaration: StrictMode runs the cleanup
  // and then re-mounts, and a flag only ever cleared would stay false.
  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  const handlePress = (e: PressEvent) => {
    if (status !== "idle") return;

    const token = ++pressTokenRef.current;
    const isCurrent = () =>
      isMountedRef.current && pressTokenRef.current === token;
    const showCheck = () => {
      if (!isCurrent()) return;
      setStatus("checked");
      setTimeout(() => {
        if (!isCurrent()) return;
        setStatus("idle");
      }, 2000);
    };

    const result = onPress?.(e);
    if (result instanceof Promise) {
      setStatus("pending");
      void result.then(showCheck, () => {
        if (!isCurrent()) return;
        setStatus("idle");
      });
      return;
    }

    showCheck();
  };
  return (
    <Button
      {...props}
      // `relative` comes first so a caller positioning the button wins; an
      // absolute button is still a containing block for the check overlay.
      className={cn(
        "relative",
        className,
        status !== "idle" && "pointer-events-none",
      )}
      onPress={handlePress}
    >
      <span
        className={cn(
          "inline-flex items-center gap-2",
          status === "pending" && "animate-pulse",
        )}
      >
        {children}
      </span>

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
