import { Alert, Button, Progress } from '@shinkai_network/shinkai-ui';
import { PartyIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { AnimatePresence, motion } from 'framer-motion';
import { Loader2, XIcon } from 'lucide-react';
import React, { useCallback, useState } from 'react';

import {
  type UpdateState,
  useCheckUpdateQuery,
  useDownloadUpdateMutation,
  useUpdateStateQuery,
} from '../../lib/updater/updater-client';
import { showAnimation } from '../../pages/layout/main-layout';

const CHECK_UPDATE_INTERVAL_MS = 30 * 60 * 1000;

const UpdateStateUI: React.FC<{
  updateState: UpdateState['state'];
  downloadProgressPercent: number;
}> = ({ updateState, downloadProgressPercent }) => {
  switch (updateState) {
    case 'available':
      return (
        <div className="flex flex-row items-center gap-2">
          <PartyIcon className="size-5" />
          <AnimatePresence>
            <motion.span
              animate="show"
              className="text-sm font-medium whitespace-nowrap"
              exit="hidden"
              initial="hidden"
              variants={showAnimation}
            >
              Update available
            </motion.span>
          </AnimatePresence>
        </div>
      );
    case 'downloading':
      return (
        <div className="flex w-full flex-col items-center justify-center gap-2.5">
          <div className="flex w-full items-center justify-between gap-2">
            <span className="text-text-secondary text-xs">
              Downloading update
            </span>
            <span className="text-text-tertiary text-xs">
              {downloadProgressPercent} %
            </span>
          </div>
          <Progress
            className="h-2 w-full rounded-lg bg-cyan-900 [&>div]:bg-cyan-400"
            max={100}
            value={downloadProgressPercent}
          />
        </div>
      );
    case 'restarting':
      return (
        <div className="flex flex-row items-center space-x-1">
          <Loader2 className="h-5 w-5 shrink-0 animate-spin" />
        </div>
      );
    default:
      return null;
  }
};

UpdateStateUI.displayName = 'UpdateStateUI';

const UpdateBanner: React.FC<{
  className?: string;
}> = ({ className }) => {
  const { data: updateState } = useUpdateStateQuery();
  const { mutateAsync: downloadUpdate } = useDownloadUpdateMutation();
  const [updateDismissed, setUpdateDismissed] = useState(false);

  useCheckUpdateQuery({
    refetchInterval: CHECK_UPDATE_INTERVAL_MS,
  });

  const downloadAndInstall = useCallback(async (): Promise<void> => {
    if (updateState?.state === 'available') {
      await downloadUpdate();
    }
  }, [updateState?.state, downloadUpdate]);

  if (!updateState?.update?.available || updateDismissed) {
    return null;
  }

  return (
    <Alert
      className={cn(
        'border-divider flex w-full max-w-md cursor-pointer flex-col rounded-2xl border p-3.5 pr-4.5',
        'z-max bg-bg-dark hover:bg-bg-default fixed top-8 left-1/2 -translate-x-1/2 shadow-2xl',
        className,
      )}
      variant="download"
      onClick={downloadAndInstall}
    >
      <div className="text-text-default flex w-full flex-col space-y-1">
        <UpdateStateUI
          downloadProgressPercent={
            updateState.downloadState?.data?.downloadProgressPercent || 0
          }
          updateState={updateState.state}
        />
        {updateState.state === 'available' && (
          <div className="text-text-secondary pl-[28px] text-xs">
            <span>Click to install the latest version. </span>
            <span className="">This will restart the application.</span>
          </div>
        )}
        {updateState.state === 'available' && (
          <Button
            className="text-text-tertiary absolute top-2 right-2 p-2"
            onClick={(e) => {
              e.stopPropagation();
              setUpdateDismissed(true);
            }}
            size="auto"
            variant="tertiary"
          >
            <XIcon className="size-4" />
          </Button>
        )}
      </div>
    </Alert>
  );
};

UpdateBanner.displayName = 'UpdateBanner';

export { UpdateBanner };
