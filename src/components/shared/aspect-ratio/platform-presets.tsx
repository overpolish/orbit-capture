// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  SiInstagram,
  SiTiktok,
  SiYoutube,
} from "@icons-pack/react-simple-icons";

import { CheckOnClickButton } from "../check-on-click-button/check-on-click-button";

export function PlatformPresets({
  onInstagram,
  onTiktok,
  onYoutube,
}: {
  onInstagram: () => void;
  onTiktok: () => void;
  onYoutube: () => void;
}) {
  const buttonProps: React.ComponentProps<typeof CheckOnClickButton> = {
    blur: "xs",
    showFocus: false,
    size: "sm",
    variant: "ghost",
  };

  return (
    <div className="flex flex-row items-center">
      <CheckOnClickButton {...buttonProps} onPress={onYoutube}>
        <SiYoutube size={20} />
      </CheckOnClickButton>
      <CheckOnClickButton {...buttonProps} onPress={onInstagram}>
        <SiInstagram size={16} />
      </CheckOnClickButton>
      <CheckOnClickButton {...buttonProps} onPress={onTiktok}>
        <SiTiktok size={14} />
      </CheckOnClickButton>
    </div>
  );
}
