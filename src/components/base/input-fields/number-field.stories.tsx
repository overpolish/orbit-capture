import { Meta, StoryObj } from "@storybook/react";
import { Ruler } from "lucide-react";

import { NumberField } from "./number-field";

const sizes: React.ComponentProps<typeof NumberField>["size"][] = [
  "md",
  "sm",
] as const;
const variants: React.ComponentProps<typeof NumberField>["variant"][] = [
  "solid",
  "line",
] as const;

const meta = {
  argTypes: {
    centered: { control: "boolean" },
    showSteppers: { control: "boolean" },
    size: {
      control: "inline-radio",
      options: sizes,
      table: { defaultValue: { summary: "md" } },
    },
    variant: {
      control: "inline-radio",
      options: variants,
      table: { defaultValue: { summary: "solid" } },
    },
  },
  args: {
    centered: false,
    defaultValue: 5,
    label: "Amount",
    maxValue: 100,
    minValue: 0,
    step: 1,
  },
  component: NumberField,
  parameters: {
    controls: {
      exclude: ["className", "leftSection", "rightSection"],
    },
    layout: "centered",
  },
  title: "Number Field",
} satisfies Meta<typeof NumberField>;

export default meta;

type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {
  args: {
    showSteppers: true,
    size: "md",
    variant: "solid",
  },
};

export const Sizes: Story = {
  parameters: {
    controls: { disable: true },
  },
  render: (args) => (
    <div className="flex gap-2 items-center">
      {sizes.map((size) => (
        <NumberField key={size} size={size} {...args} />
      ))}
    </div>
  ),
};

export const Variants: Story = {
  parameters: {
    controls: { disable: true },
  },
  render: (args) => (
    <div className="flex gap-2 items-center">
      {variants.map((variant) => (
        <NumberField key={variant} variant={variant} {...args} />
      ))}
    </div>
  ),
};

export const WithoutSteppers: Story = {
  args: {
    label: undefined,
    showSteppers: false,
  },
  parameters: { controls: { disable: true } },
};

export const Sections: Story = {
  args: {
    leftSection: <Ruler size={18} />,
    rightSection: <span className="text-xs">px</span>,
    showSteppers: false,
    variant: "solid",
  },
  parameters: { controls: { include: ["showSteppers", "variant"] } },
};
