import { Check, Sparkle } from "lucide-react";
import { ComponentProps, ReactNode } from "react";
import { TooltipTrigger } from "react-aria-components";
import { twMerge } from "tailwind-merge";

import { Badge } from "../../components/base/badge/badge";
import { Button } from "../../components/base/button/button";
import { Sparkles } from "../../components/base/sparkles/sparkles";
import { Tooltip } from "../../components/base/tooltip/tooltip";

import { openPermissionSettings, requestPermission } from "./api";
import { PermissionKind, PermissionStatus } from "./types";

const sparkles = {
  colors: ["#FFFFFF"],
  duration: { max: 2.5, min: 0.5 },
  icon: Sparkle,
  offset: { x: { max: 50, min: -10 }, y: { max: 50, min: -10 } },
  opacity: 0.4,
  scale: { max: 0.5, min: 0.2 },
  sparklesCount: 2,
} satisfies ComponentProps<typeof Sparkles>;

type PermissionRowProps = {
  color: string;
  icon: ReactNode;
  permission: PermissionKind;
  status: PermissionStatus;
  title: string;
  description?: string;
  isOptional?: boolean;
};

export function PermissionRow({
  color,
  description,
  icon,
  isOptional,
  permission,
  status,
  title,
}: PermissionRowProps) {
  const grant = () => {
    const action = status.canRequest
      ? requestPermission(permission)
      : openPermissionSettings(permission);
    void action;
  };

  return (
    <div className="flex items-center gap-4">
      <div
        className={twMerge(
          "flex size-16 items-center justify-center rounded-2xl text-white",
          color,
        )}
      >
        {icon}
      </div>
      <div className="flex grow flex-col text-content-fg">
        <div className="flex items-center gap-2">
          <span className="font-semibold">{title}</span>
          {isOptional ? <Badge size="sm">Optional</Badge> : null}
        </div>
        {description ? (
          <span className="text-sm text-muted">{description}</span>
        ) : null}
      </div>

      {status.granted ? (
        <div className="flex w-[62px] justify-center">
          <Sparkles {...sparkles}>
            <Check className="text-success" size={32} />
          </Sparkles>
        </div>
      ) : (
        <TooltipTrigger isDisabled={status.canRequest}>
          <Button onPress={grant} shiny>
            {status.canRequest ? "Grant" : "Open System Settings"}
          </Button>
          <Tooltip size="sm">Enable manually</Tooltip>
        </TooltipTrigger>
      )}
    </div>
  );
}
