import {
  Button,
  Dialog,
  DialogContent,
  Input,
} from '@shinkai_network/shinkai-ui';
import { useVirtualizer } from '@tanstack/react-virtual';
import { openPath } from '@tauri-apps/plugin-opener';
import { DownloadIcon, Loader2, RotateCw, Search } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';

import {
  useDownloadTauriLogsMutation,
  useRetrieveLogsQuery,
} from '../../../lib/shinkai-logs/logs-client';
import { LogIcon, LogItem } from './log-entry';

export const Logs = () => {
  const {
    data: tauriLogs,
    refetch: refetchTauriLogs,
    isLoading: tauriLogsLoading,
  } = useRetrieveLogsQuery();

  const [searchQuery, setSearchQuery] = useState('');
  const [selectedLog, setSelectedLog] = useState<string | null>(null);

  const filteredLogs = useMemo(
    () =>
      searchQuery
        ? (tauriLogs || []).filter((log) =>
            log.message.toLowerCase().includes(searchQuery.toLowerCase()),
          )
        : tauriLogs || [],
    [tauriLogs, searchQuery],
  );

  const { mutate: downloadTauriLogs, isPending: downloadTauriLogsIsPending } =
    useDownloadTauriLogsMutation({
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

  const virtualizerParentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: filteredLogs?.length ?? 0,
    getScrollElement: () => virtualizerParentRef.current,
    estimateSize: () => 50,
    overscan: 20,
  });

  useEffect(() => {
    if (filteredLogs?.length) {
      virtualizer.scrollToIndex(filteredLogs?.length - 1, {
        behavior: 'auto',
      });
    }
  }, [filteredLogs, virtualizer]);

  const refetchLogs = async (): Promise<void> => {
    await refetchTauriLogs();
  };

  return (
    <div className="h-full overflow-hidden">
      <div className="bg-background sticky top-0 z-10 mb-2 flex items-center gap-2 p-2">
        <div className="relative flex items-center">
          <Search className="text-text-secondary absolute left-3 h-4 w-4" />
          <Input
            className="placeholder-gray-80 !h-full bg-transparent py-2 pl-10"
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search logs..."
            value={searchQuery}
          />
        </div>
      </div>
      <div
        className="relative h-[calc(100%-4rem)] overflow-y-scroll"
        ref={virtualizerParentRef}
      >
        <div
          className="relative z-0"
          style={{
            height: `${virtualizer.getTotalSize()}px`,
            width: '100%',
            position: 'relative',
          }}
        >
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const log = filteredLogs?.[virtualRow.index];
            return (
              <div
                className="text-text-secondary absolute top-0 left-0 w-full overflow-hidden border-b border-gray-100/10 font-mono text-xs leading-relaxed"
                key={virtualRow.index}
                style={{
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                  zIndex: virtualRow.index,
                }}
              >
                {log && (
                  <LogItem
                    log={log}
                    onLogClick={(message) => setSelectedLog(message)}
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
      <div className="fixed right-12 bottom-6 flex gap-2">
        <Button
          disabled={tauriLogsLoading}
          onClick={() => refetchLogs()}
          rounded="lg"
          size="sm"
          type="button"
          variant="default"
        >
          {tauriLogsLoading ? (
            <Loader2 className="animate-spin" />
          ) : (
            <RotateCw className="size-3.5" />
          )}
        </Button>
        <Button
          disabled={downloadTauriLogsIsPending}
          onClick={() => downloadTauriLogs()}
          rounded="lg"
          size="sm"
          type="button"
          variant="default"
        >
          {downloadTauriLogsIsPending ? (
            <Loader2 className="animate-spin" />
          ) : (
            <DownloadIcon className="ml-2 size-3.5" />
          )}
          Download Logs
        </Button>
      </div>

      <Dialog onOpenChange={() => setSelectedLog(null)} open={!!selectedLog}>
        <DialogContent className="max-h-[90vh] w-[90vw] max-w-[800px] overflow-y-auto">
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <LogIcon level="INFO" />
                <span className="text-text-secondary text-sm font-medium">
                  Log Details
                </span>
              </div>
              <Button
                onClick={() => navigator.clipboard.writeText(selectedLog || '')}
                size="sm"
                variant="tertiary"
              >
                Copy to clipboard
              </Button>
            </div>
            <div className="rounded-lg bg-gray-900 p-4">
              <pre className="text-text-secondary font-mono text-sm leading-relaxed break-words whitespace-pre-wrap sm:text-base">
                {selectedLog}
              </pre>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
};
