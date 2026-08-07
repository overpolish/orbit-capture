import { X } from "lucide-react";
import { motion, MotionProps } from "motion/react";
import { use } from "react";
import {
  Button as AriaButton,
  SelectStateContext,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import { elementFocusVisible, focusStyles } from "../../../../lib/styling";
import { tv } from "../../../../lib/variants";

const clearButtonVariants = tv({
  slots: {
    base: "flex items-center absolute inset-y-0 flex right-7",
    button: [
      "transition-colors rounded-sm p-0.5 mb-0.5 flex",
      "data-[hovered]:bg-error/10 data-[hovered]:text-error",
      "data-[pressed]:bg-error/5",
      focusStyles,
      elementFocusVisible,
    ],
  },
});

const MotionAriaButton = motion.create(AriaButton);

type ClearButtonProps = MotionProps &
  VariantProps<typeof clearButtonVariants> & {
    className?: string;
    onClear?: () => void;
    size?: number;
  };

export const ClearButton = ({
  className,
  onClear,
  size = 14,
  ...props
}: ClearButtonProps) => {
  const { base, button } = clearButtonVariants({ className });
  const state = use(SelectStateContext);

  if (!state?.selectedItems.length) return null;

  return (
    <div className={base()}>
      <MotionAriaButton
        {...props}
        aria-label="Clear selection"
        className={button({ className })}
        onPress={() => {
          state.setValue(null);
          if (onClear) onClear();
        }}
        slot={null}
      >
        <X className="translate-x-0" size={size} />
      </MotionAriaButton>
    </div>
  );
};
