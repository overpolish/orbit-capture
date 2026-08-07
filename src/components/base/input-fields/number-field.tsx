import { Minus, Plus } from "lucide-react";
import {
  Button,
  Group,
  Input,
  NumberField as AriaNumberField,
  NumberFieldProps as AriaNumberFieldProps,
  Label,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";
import { inputFieldVariants } from "../input-fields/input-field";

const ICON_SIZES = {
  md: 16,
  sm: 14,
};

const numberFieldVariants = tv({
  extend: inputFieldVariants,
  slots: {
    stepper: [
      "text-muted/75 px-3 self-stretch transition-colors",
      "data-[hovered]:text-content-fg",
    ],
  },
});

export type NumberFieldProps = AriaNumberFieldProps &
  VariantProps<typeof numberFieldVariants> & {
    centered?: boolean;
    className?: string;
    label?: string;
    leftSection?: React.ReactNode;
    rightSection?: React.ReactNode;
    showSteppers?: boolean;
    size?: "sm" | "md";
    variant?: "line" | "solid";
  };

export const NumberField = ({
  centered,
  className,
  label,
  leftSection,
  rightSection,
  showSteppers = true,
  size,
  variant,
  ...props
}: NumberFieldProps) => {
  const {
    base,
    field,
    input,
    inputWrapper,
    label: _label,
    line,
    stepper,
  } = numberFieldVariants({ centered, size, variant });

  return (
    <AriaNumberField {...props} className={base({ className })}>
      {label && <Label className={_label()}>{label}</Label>}

      <Group className={field()}>
        {showSteppers && (
          <Button aria-label="Decrement" className={stepper()} slot="decrement">
            <Minus size={size ? ICON_SIZES[size] : 16} />
          </Button>
        )}

        <div className={inputWrapper()}>
          {leftSection}

          <Input className={input()} />

          {rightSection}
        </div>

        {showSteppers && (
          <Button aria-label="Increment" className={stepper()} slot="increment">
            <Plus size={size ? ICON_SIZES[size] : 16} />
          </Button>
        )}

        {variant === "line" && <div className={line()} />}
      </Group>
    </AriaNumberField>
  );
};
