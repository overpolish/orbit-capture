// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { AnimatePresence, motion, MotionProps } from "motion/react";
import { ReactNode, useEffect, useRef } from "react";

import { cn } from "../../../lib/styling";

type ContentRotateProps = MotionProps & {
  children: ReactNode;
  contentKey: string;
  className?: string;
  containerClassName?: string;
};
export const ContentRotate = ({
  children,
  className,
  containerClassName,
  contentKey,
  ...props
}: ContentRotateProps) => {
  const isFirstMountRef = useRef(true);

  useEffect(() => {
    isFirstMountRef.current = false;
  }, []);

  return (
    <div className={cn("overflow-hidden", containerClassName)}>
      <AnimatePresence mode="wait">
        <motion.div
          animate={{ opacity: 1, y: 0 }}
          className={className}
          exit={{ opacity: 0, y: 25 }}
          initial={isFirstMountRef.current ? false : { opacity: 0, y: -25 }}
          key={contentKey}
          transition={{ duration: 0.175, ease: "easeOut" }}
          {...props}
        >
          {children}
        </motion.div>
      </AnimatePresence>
    </div>
  );
};
