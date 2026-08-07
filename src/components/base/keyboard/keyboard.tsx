import type { HTMLAttributes, Ref } from "react";

import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";

const keyboardVariants = tv({
  base: "flex items-center rounded-sm tracking-wider font-sans",
  defaultVariants: {
    size: "md",
    variant: "default",
  },
  variants: {
    size: {
      md: "gap-0.5 px-1 text-sm",
      sm: "gap-0.25 px-0.5 text-xs",
      xs: "px-0.5 text-xxs",
    },
    variant: {
      default: "bg-neutral text-muted",
      ghost: "bg-transparent text-muted",
    },
  },
});

type KeyboardProps = HTMLAttributes<HTMLElement> &
  VariantProps<typeof keyboardVariants> & { ref?: Ref<HTMLElement> };

export const Keyboard = ({
  children,
  className,
  ref,
  size,
  variant,
  ...rest
}: KeyboardProps) => {
  return (
    <kbd
      {...rest}
      className={keyboardVariants({ className, size, variant })}
      ref={ref}
    >
      {children}
    </kbd>
  );
};
