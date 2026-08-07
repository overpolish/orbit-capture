import { Disc, Mic, PersonStanding, Video } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";

import logoUrl from "../../assets/orbit-capture-mark.svg";
import { Button } from "../../components/base/button/button";

import { restartApp } from "./api";
import { PermissionRow } from "./permission-row";
import { usePermissionStore } from "./store";

const ICON_SIZE = 40;
const gradients = {
  blue: "bg-linear-0 from-[#3B83F7] from-20% to-[#5DA3F8]",
  gray: "bg-linear-0 from-[#98989D] from-20% to-[#C0C0C4]",
  red: "bg-linear-0 from-[#EB5545] from-20% to-[#EE8176]",
};

export function PermissionsWindow() {
  const permissions = usePermissionStore((state) => state.permissions);
  const hasRequired =
    permissions.accessibility.granted && permissions.screenRecording.granted;

  return (
    <main className="min-h-screen overflow-hidden rounded-[10px] bg-content/92 p-8">
      <div className="mb-5 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <img
            alt="Orbit Capture"
            className="size-7 shrink-0 brightness-0 dark:invert"
            draggable={false}
            src={logoUrl}
          />
          <h1 className="m-0 animate-gradient bg-linear-to-r from-orange-400 to-orange-500 bg-clip-text bg-size-[300%] text-3xl font-bold text-transparent">
            Permissions
          </h1>
        </div>

        <AnimatePresence>
          {hasRequired ? (
            <motion.div
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0 }}
              initial={{ opacity: 0, scale: 0 }}
            >
              <Button
                color="info"
                onPress={() => void restartApp()}
                size="sm"
                variant="soft"
              >
                Restart Orbit Capture
              </Button>
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>

      <div className="flex flex-col gap-4">
        <PermissionRow
          color={gradients.blue}
          description="For capturing cursor events."
          icon={<PersonStanding size={ICON_SIZE} />}
          permission="accessibility"
          status={permissions.accessibility}
          title="Accessibility"
        />
        <PermissionRow
          color={gradients.red}
          icon={<Disc size={ICON_SIZE} />}
          permission="screenRecording"
          status={permissions.screenRecording}
          title="Screen Recording"
        />
        <PermissionRow
          color={gradients.gray}
          icon={<Video size={ICON_SIZE} />}
          isOptional
          permission="camera"
          status={permissions.camera}
          title="Camera"
        />
        <PermissionRow
          color={gradients.gray}
          icon={<Mic size={ICON_SIZE} />}
          isOptional
          permission="microphone"
          status={permissions.microphone}
          title="Microphone"
        />
      </div>
    </main>
  );
}
