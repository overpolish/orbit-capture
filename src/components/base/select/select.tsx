import type { ReactNode, Ref } from "react";

import { clsx } from "clsx";
import { ChevronDown } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import {
  Select as AriaSelect,
  SelectProps as AriaSelectProps,
  Button,
  Label,
  Popover,
  SelectValue,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import {
  availableVariants,
  elementFocusVisible,
  focusStyles,
} from "../../../lib/styling";
import { tv } from "../../../lib/variants";
import { ListBox } from "../listbox/listbox";

import { ClearButton } from "./components/clear-button";

const ICON_SIZES = {
  md: 16,
  sm: 14,
};

const selectVariants = tv({
  compoundVariants: [
    {
      class: { trigger: "py-0.5" },
      compact: true,
      size: "md",
      variant: "line",
    },
    {
      class: { trigger: "py-0.5" },
      compact: true,
      size: "sm",
      variant: "line",
    },
  ],
  defaultVariants: {
    showFocus: true,
    size: "md",
    variant: "solid",
  },
  slots: {
    base: "flex flex-col gap-1",
    controls: "text-muted/75",
    label: "text-muted font-medium",
    line: [
      "absolute bottom-0 inset-x-0 bg-transparent h-[2px] pointer-events-none transition-shadow shadow-[0_1px_0_0] shadow-muted/30",
      "group-data-[hovered]:shadow-[0_2px_0_0] group-data-[hovered]:shadow-content-fg/75",
      "group-data-[pressed]:shadow-[0_2px_0_0] group-data-[pressed]:shadow-content-fg/75",
    ],
    trigger: [
      "group relative outline-none shrink inline-flex flex-row items-center justify-between text-content-fg gap-4 rounded-md transition-colors",
      "data-[hovered]:bg-neutral/50",
      focusStyles,
    ],
  },
  variants: {
    compact: availableVariants("true"),
    showFocus: {
      true: {
        trigger: elementFocusVisible,
      },
    },
    size: {
      md: { label: "text-sm", trigger: "text-sm px-2 py-2" },
      sm: { label: "text-xs", trigger: "text-xs px-2 py-2" },
    },
    variant: {
      ghost: {
        trigger: "px-2 py-1",
      },
      line: {
        trigger: "data-[hovered]:bg-transparent",
      },
      solid: {
        trigger: "bg-content border-1 border-muted/30",
      },
    },
  },
});

type SelectProps<T extends object> = Omit<AriaSelectProps<T>, "children"> &
  VariantProps<typeof selectVariants> & {
    children?: ReactNode | ((item: T) => ReactNode);
    className?: string;
    clearable?: boolean;
    items?: Iterable<T>;
    label?: string;
    leftSection?: ReactNode;
    onClear?: () => void;
    onPress?: () => void;
    /**
     * @default false
     * @type boolean
     * @description
     * You'll need to provide and control the visibility of your own
     * listbox
     */
    standalone?: boolean;
    triggerRef?: Ref<HTMLButtonElement>;
  };

export const Select = <T extends object>({
  children,
  className,
  clearable = true,
  compact,
  items,
  label,
  leftSection,
  onClear,
  onPress,
  placeholder,
  showFocus,
  size,
  standalone,
  triggerRef,
  variant,
  ...props
}: SelectProps<T>) => {
  const {
    base,
    controls,
    label: _label,
    line,
    trigger,
  } = selectVariants({ compact, size, variant });

  return (
    <AriaSelect {...props} className={base()}>
      {({ isOpen }) => (
        <>
          {label && <Label className={_label()}>{label}</Label>}

          <div className="relative">
            <Button
              className={trigger({ className, showFocus })}
              onPress={onPress}
              ref={triggerRef}
            >
              <div className="inline-flex flex-row items-center gap-2 flex-1 min-w-0">
                {leftSection != null && <div>{leftSection}</div>}

                <SelectValue className="data-[placeholder]:text-muted/75 truncate">
                  {({ defaultChildren, isPlaceholder }) =>
                    isPlaceholder ? placeholder : defaultChildren
                  }
                </SelectValue>
              </div>

              <motion.div
                animate={{
                  rotate: isOpen ? 180 : 0,
                  y: isOpen ? -0.5 : 0,
                }}
                aria-hidden="true"
                className={controls({ className: clearable && "ml-3" })}
                transition={{
                  duration: 0.2,
                }}
              >
                <ChevronDown size={size ? ICON_SIZES[size] : 16} />
              </motion.div>

              {variant === "line" && <div className={line()} />}
            </Button>

            <AnimatePresence>
              {clearable && (
                <ClearButton
                  animate={{ opacity: 1 }}
                  className={controls()}
                  exit={{
                    opacity: 0,
                    scale: 0,
                  }}
                  initial={{ opacity: 0 }}
                  onClear={onClear}
                  size={12}
                />
              )}
            </AnimatePresence>
          </div>

          <Popover
            className={({ placement }) =>
              clsx(
                isOpen ? "animate-in fade-in" : "animate-out fade-out",
                isOpen
                  ? placement === "bottom"
                    ? "slide-in-from-top-5"
                    : "slide-in-from-bottom-5"
                  : placement === "bottom"
                    ? "slide-out-to-top-5"
                    : "slide-out-to-bottom-5",
              )
            }
            // Standalone still needs listbox to be rendered to show a display value
            isOpen={standalone ? false : undefined}
          >
            <ListBox items={items}>{children}</ListBox>
          </Popover>
        </>
      )}
    </AriaSelect>
  );
};
