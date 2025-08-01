import { ReloadIcon } from '@radix-ui/react-icons';
import { useGetHealth } from '@shinkai_network/shinkai-node-state/v2/queries/getHealth/useGetHealth';
import { Button } from '@shinkai_network/shinkai-ui';
import { openPath } from '@tauri-apps/plugin-opener';
import { DownloadIcon, Loader2 } from 'lucide-react';
import React, { useState } from 'react';
import { toast } from 'sonner';

import { ResetConnectionDialog } from '../components/reset-connection-dialog';
import { useAuth } from '../store/auth';
import { useShinkaiNodeManager } from '../store/shinkai-node-manager';
import { useDownloadTauriLogsMutation } from './shinkai-logs/logs-client';
import { useShinkaiNodeIsRunningQuery } from './shinkai-node-manager/shinkai-node-manager-client';
import { openShinkaiNodeManagerWindow } from './shinkai-node-manager/shinkai-node-manager-windows-utils';

export const ShinkaiNodeRunningOverlay = ({
  children,
}: {
  children: React.ReactNode;
}) => {
  const auth = useAuth((store) => store.auth);
  const { data: isShinkaiNodeRunning, isPending: isShinkaiNodeRunningPending } =
    useShinkaiNodeIsRunningQuery();
  const isInUse = useShinkaiNodeManager((store) => store.isInUse);

  const {
    isSuccess: isHealthSuccess,
    data: health,
    isPending: isHealthPending,
    error: healthError,
    isError: isHealthError,
  } = useGetHealth(
    { nodeAddress: auth?.node_address ?? '' },
    { refetchInterval: 35000 },
  );

  const { mutate: downloadTauriLogs } = useDownloadTauriLogsMutation({
    onSuccess: (result) => {
      toast.success('Logs downloaded successfully', {
        description: `You can find the logs file in your downloads folder`,
        action: {
          label: 'Open',
          onClick: async () => {
            await openPath(result.savePath);
          },
        },
      });
    },
    onError: (error) => {
      toast.error('Failed to download logs', {
        description: error.message,
      });
    },
  });

  const [isResetConnectionDialogOpen, setIsResetConnectionDialogOpen] =
    useState(false);

  const isShinkaiNodeHealthy = isHealthSuccess && health.status === 'ok';

  if (isHealthPending || isShinkaiNodeRunningPending) {
    return (
      <div className="flex size-full flex-col items-center justify-center gap-3 py-8">
        <Loader2 className="h-8 w-8 animate-spin text-white" />
        <span className="text-text-secondary text-sm">
          Checking Shinkai Node Status ...
        </span>
      </div>
    );
  }

  if (isHealthError) {
    return (
      <div className="flex size-full items-center justify-center">
        <div
          className="flex flex-col items-center gap-6 px-3 py-4 text-sm"
          role="alert"
        >
          <div className="space-y-2 text-center text-red-400">
            <p>Unable to connect to Shinkai Node.</p>
            <pre className="px-4 whitespace-break-spaces">
              {healthError.message}
            </pre>
          </div>
          <div className="flex items-center gap-4">
            <Button
              className="min-w-[140px]"
              onClick={() => {
                downloadTauriLogs();
              }}
              size="sm"
              type="button"
              variant="outline"
            >
              <DownloadIcon className="size-3.5" />
              Download Logs
            </Button>
            <Button
              className="min-w-[140px]"
              onClick={() => setIsResetConnectionDialogOpen(true)}
              size="sm"
              type="button"
            >
              <ReloadIcon className="size-3.5" />
              Reset App
            </Button>
            <ResetConnectionDialog
              allowClose
              isOpen={isResetConnectionDialogOpen}
              onOpenChange={setIsResetConnectionDialogOpen}
            />
          </div>
        </div>
      </div>
    );
  }

  if (isShinkaiNodeHealthy && !!auth) {
    return children;
  }

  return (
    <div className="flex h-screen flex-col items-center justify-center gap-10">
      <div className="mx-auto flex max-w-lg flex-col items-center gap-4">
        <span className="text-4xl">⚠️</span>
        <h1 className="text-3xl font-bold">
          Unable to Connect to Shinkai Node
        </h1>
        <p className="text-text-secondary text-base">
          Please make sure the Shinkai Node is running and try again.
        </p>
      </div>
      {isInUse && !isShinkaiNodeRunning && (
        <Button
          onClick={async () => {
            await openShinkaiNodeManagerWindow();
          }}
        >
          Launch Shinkai Node
        </Button>
      )}
    </div>
  );
};
