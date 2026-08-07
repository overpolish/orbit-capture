import {
  Info,
  PencilLine,
  SquareBottomDashedScissors,
  SquareDot,
} from "lucide-react";

import { AspectRatio } from "../../components/shared/aspect-ratio/aspect-ratio";
import { CheckOnClickButton } from "../../components/shared/check-on-click-button/check-on-click-button";

import {
  centerWindow,
  makeWindowBorderless,
  resizeWindow,
  restoreWindowBorder,
} from "./api";
import { WindowDetails } from "./types";

type WindowUtilitiesProps = {
  selectedWindow: WindowDetails | null;
};

const isWindows = navigator.userAgent.includes("Windows");

export function WindowUtilities({ selectedWindow }: WindowUtilitiesProps) {
  const run = (action: (window: WindowDetails) => Promise<null>) => {
    if (selectedWindow) void action(selectedWindow);
  };

  return (
    <div className="relative flex shrink-0 flex-col items-center gap-1 px-1">
      <AspectRatio
        onApply={(width, height) => {
          if (selectedWindow) void resizeWindow(selectedWindow, width, height);
        }}
      />
      <span className="flex items-center gap-1 text-xxs font-extralight text-muted">
        <Info size={10} /> Applications may impose their own sizing
        restrictions.
      </span>

      <div className="absolute left-1 top-0">
        <CheckOnClickButton
          isDisabled={!selectedWindow}
          onPress={() => {
            run(centerWindow);
          }}
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          <SquareDot size={14} />
          Center
        </CheckOnClickButton>
      </div>

      {isWindows ? (
        <div className="absolute right-1 top-0 flex flex-col items-end gap-1">
          <CheckOnClickButton
            isDisabled={!selectedWindow}
            onPress={() => {
              run(makeWindowBorderless);
            }}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            <SquareBottomDashedScissors size={14} />
            Borderless
          </CheckOnClickButton>
          <CheckOnClickButton
            isDisabled={!selectedWindow}
            onPress={() => {
              run(restoreWindowBorder);
            }}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            <PencilLine size={14} />
            Restore border
          </CheckOnClickButton>
        </div>
      ) : null}
    </div>
  );
}
