// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { SearchSlash } from "lucide-react";
import { Ref } from "react";
import {
  ListBox as AriaListBox,
  ListBoxProps as AriaListBoxProps,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";

const listBoxVariants = tv({
  base: [
    "flex flex-col gap-2 bg-content text-content-fg w-(--trigger-width) rounded-md shadow-md border-1 border-muted/30 p-1 overflow-auto outline-none",
    "scroll-py-5",
    "data-[empty]:py-4 data-[empty]:text-xs data-[empty]:text-muted data-[empty]:flex data-[empty]:flex-row data-[empty]:items-center data-[empty]:justify-center",
  ],
});

type ListBoxProps<T extends object> = AriaListBoxProps<T> &
  VariantProps<typeof listBoxVariants> & {
    className?: string;
    ref?: Ref<HTMLDivElement>;
  };

export const ListBox = <T extends object>({
  children,
  className,
  ref,
  ...props
}: ListBoxProps<T>) => {
  return (
    <AriaListBox
      ref={ref}
      renderEmptyState={() => (
        <>
          <SearchSlash size={16} />
          No items found.
        </>
      )}
      {...props}
      className={listBoxVariants({ className })}
    >
      {children}
    </AriaListBox>
  );
};
