// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CircleAlert, CircleCheck, Info, TriangleAlert } from "lucide-react";
import { ComponentProps, ReactNode } from "react";
import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";

const alertVariants = tv({
  defaultVariants: {
    color: "neutral",
    size: "md",
  },
  slots: {
    base: "flex items-start rounded-md border text-content-fg",
    content: "min-w-0 leading-relaxed",
    icon: "mt-px shrink-0 [&>svg]:size-full",
  },
  variants: {
    color: {
      error: {
        base: "border-error/25 bg-error/10",
        icon: "text-error",
      },
      info: {
        base: "border-info/25 bg-info/10",
        icon: "text-info",
      },
      neutral: {
        base: "border-muted/20 bg-neutral/20",
        icon: "text-muted",
      },
      success: {
        base: "border-success/25 bg-success/10",
        icon: "text-success",
      },
      warning: {
        base: "border-warning/25 bg-warning/10",
        icon: "text-warning",
      },
    },
    size: {
      md: {
        base: "gap-2.5 px-2.5 py-2 text-xs",
        icon: "size-4",
      },
      sm: {
        base: "gap-2 px-2 py-2 text-xxs",
        icon: "size-3.5",
      },
    },
  },
});

const defaultIcons = {
  error: CircleAlert,
  info: Info,
  neutral: Info,
  success: CircleCheck,
  warning: TriangleAlert,
};

type AlertProps = Omit<ComponentProps<"div">, "color"> &
  VariantProps<typeof alertVariants> & {
    icon?: ReactNode | false;
  };

export function Alert({
  children,
  className,
  color = "neutral",
  icon: customIcon,
  size,
  ...props
}: AlertProps) {
  const { base, content, icon } = alertVariants({
    className,
    color,
    size,
  });
  const DefaultIcon = defaultIcons[color];

  return (
    <div className={base()} {...props}>
      {customIcon === false ? null : (
        <span aria-hidden="true" className={icon()}>
          {customIcon ?? <DefaultIcon />}
        </span>
      )}
      <div className={content()}>{children}</div>
    </div>
  );
}
