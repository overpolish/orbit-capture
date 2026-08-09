// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Label,
  Slider as AriaSlider,
  SliderFill,
  SliderOutput,
  type SliderProps as AriaSliderProps,
  SliderThumb,
  SliderTrack,
} from "react-aria-components";

import { elementFocusVisible, focusStyles } from "../../../lib/styling";
import { tv } from "../../../lib/variants";

const sliderVariants = tv({
  slots: {
    base: "flex w-full flex-col gap-2 text-content-fg",
    fill: "absolute h-full rounded-full bg-info",
    header: "flex items-center justify-between gap-3",
    label: "text-xs",
    output: "text-xxs text-muted tabular-nums",
    thumb: [
      "top-1/2 size-4 rounded-full border-2 border-info bg-content shadow-sm",
      "transition-[transform,box-shadow] data-[dragging]:scale-110 data-[hovered]:scale-110",
      focusStyles,
      elementFocusVisible,
    ],
    track: [
      "relative h-1.5 w-full rounded-full bg-muted/20",
      "data-[disabled]:opacity-50",
    ],
  },
});

type SliderProps = Omit<AriaSliderProps<number>, "children" | "className"> & {
  label?: React.ReactNode;
  renderValue?: (value: number) => React.ReactNode;
};

export function Slider({ label, renderValue, ...props }: SliderProps) {
  const {
    base,
    fill,
    header,
    label: labelStyle,
    output,
    thumb,
    track,
  } = sliderVariants();

  return (
    <AriaSlider {...props} className={base()}>
      {label || renderValue ? (
        <div className={header()}>
          {label ? <Label className={labelStyle()}>{label}</Label> : <span />}
          {renderValue ? (
            <SliderOutput className={output()}>
              {({ state }) => renderValue(state.getThumbValue(0))}
            </SliderOutput>
          ) : null}
        </div>
      ) : null}
      <SliderTrack className={track()}>
        <SliderFill className={fill()} />
        <SliderThumb className={thumb()} />
      </SliderTrack>
    </AriaSlider>
  );
}
