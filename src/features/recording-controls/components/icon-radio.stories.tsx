import { Meta, StoryObj } from "@storybook/react";
import { Rabbit, Snail } from "lucide-react";

import { Keyboard } from "../../../components/base/keyboard/keyboard";
import { RadioGroup } from "../../../components/base/radio-group/radio-group";

import { IconRadio } from "./icon-radio";

const meta = {
  args: {
    icon: <Rabbit />,
    shortcut: <Keyboard size="xs">⌘1</Keyboard>,
    subtext: "Fast",
    value: "fast",
  },
  component: IconRadio,
  parameters: {
    controls: { exclude: ["icon", "shortcut"] },
    layout: "centered",
  },
  title: "Start Recording/Icon Option",
} satisfies Meta<typeof IconRadio>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: (args) => (
    <RadioGroup aria-label="Speed" orientation="horizontal">
      <IconRadio {...args} />
    </RadioGroup>
  ),
};

export const Multiple: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <RadioGroup aria-label="Speed" orientation="horizontal">
      <IconRadio {...args} icon={<Rabbit />} subtext="Fast" value="fast" />
      <IconRadio icon={<Snail />} subtext="Slow" value="slow" />
    </RadioGroup>
  ),
};
