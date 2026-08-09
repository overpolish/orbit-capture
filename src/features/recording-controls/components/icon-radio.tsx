// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  RadioButton as AriaRadioButton,
  RadioField as AriaRadioField,
  RadioFieldProps as AriaRadioFieldProps,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";

const radioVariants = tv({
  slots: {
    base: "group relative flex flex-col grow items-center p-2 rounded-md transition select-none",
    icon: [
      "text-muted transition-colors",
      "group-data-[hovered]:text-content-fg/75",
      "group-data-[selected]:text-content-fg",
    ],
    subtext: [
      "text-[10px] font-semibold text-muted transition-colors",
      "group-data-[selected]:text-content-fg",
    ],
  },
});

type IconRadioProps = AriaRadioFieldProps &
  VariantProps<typeof radioVariants> & {
    icon: React.ReactNode;
    subtext: string;
    shortcut?: React.ReactNode;
  };

export const IconRadio = ({
  icon,
  shortcut,
  subtext,
  ...props
}: IconRadioProps) => {
  const { base, icon: _icon, subtext: _subtext } = radioVariants();

  return (
    <AriaRadioField {...props} className={base()}>
      <AriaRadioButton className="contents">
        <div className={_icon()}>{icon}</div>
        <div className={_subtext()}>{subtext}</div>
        {shortcut}
      </AriaRadioButton>
    </AriaRadioField>
  );
};
