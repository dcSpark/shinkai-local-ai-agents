import { ReloadIcon } from '@radix-ui/react-icons';
import { useGetHealth } from '@shinkai_network/shinkai-node-state/v2/queries/getHealth/useGetHealth';
import { Button } from '@shinkai_network/shinkai-ui';
import { listen } from '@tauri-apps/api/event';
import { openPath } from '@tauri-apps/plugin-opener';
import { AlertCircle, DownloadIcon, Loader2 } from 'lucide-react';
import React, { useEffect, useState } from 'react';
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
    refetch: refetchHealth,
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

  useEffect(() => {
    const handleFocus = () => {
      void refetchHealth();
    };

    const unlistenPromise = listen('tauri://focus', handleFocus);

    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [auth?.node_address, refetchHealth]);

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

  if (isHealthError && !isShinkaiNodeRunning && isInUse) {
    return (
      <div className="flex size-full flex-col items-center justify-center gap-6 px-8">
        <div className="flex max-w-lg flex-col items-center gap-4 text-center">
          <div className="flex size-12 items-center justify-center rounded-full bg-red-500/10">
            <AlertCircle className="size-6 text-red-400" />
          </div>
          <div className="space-y-2">
            <h1 className="text-2xl font-semibold">
              Connection Issue Detected
            </h1>
            <p className="text-text-secondary text-base">
              We're unable to connect to your Shinkai node. Simply start or
              reconnect to your node to continue.
            </p>
          </div>
        </div>
        <Button
          onClick={async () => {
            await openShinkaiNodeManagerWindow();
          }}
          size="md"
          className="min-w-[160px]"
          type="button"
        >
          Open Shinkai Node Manager
        </Button>
      </div>
    );
  }

  if (isShinkaiNodeHealthy && !!auth) {
    return children;
  }

  return (
    <div className="flex size-full items-center justify-center">
      <div
        className="flex flex-col items-center gap-6 px-3 py-4 text-sm"
        role="alert"
      >
        <div className="space-y-2 text-center text-red-400">
          <p>Unable to connect to Shinkai Node.</p>
          <pre className="px-4 whitespace-break-spaces">
            {healthError?.message ?? 'Unknown error'}
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
};
