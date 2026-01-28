/** biome-ignore-all lint/a11y/useSemanticElements: <explanation> */
/** biome-ignore-all lint/a11y/useKeyWithClickEvents: <explanation> */
import { DialogClose } from '@radix-ui/react-dialog';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils';
import { useRemoveJob } from '@shinkai_network/shinkai-node-state/v2/mutations/removeJob/useRemoveJob';
import { useRemoveJobs } from '@shinkai_network/shinkai-node-state/v2/mutations/removeMultipleJobs/useRemoveMultipleJobs';
import { useGetInboxesWithPagination } from '@shinkai_network/shinkai-node-state/v2/queries/getInboxes/useGetInboxesWithPagination';
import {
  Button,
  Checkbox,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
  ScrollArea,
  SearchInput,
  Skeleton,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { formatDistanceToNow } from 'date-fns';
import { AnimatePresence, motion } from 'framer-motion';
import {
  Edit3Icon,
  ExternalLinkIcon,
  SearchXIcon,
  Trash2Icon,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../store/auth';
import { InboxNameInput } from './inbox-name-input';

type ManageChatsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function ManageChatsDialog({
  open,
  onOpenChange,
}: ManageChatsDialogProps) {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();

  const [searchQuery, setSearchQuery] = useState('');
  const [selectedInboxIds, setSelectedInboxIds] = useState<Set<string>>(
    new Set(),
  );
  const [lastSelectedId, setLastSelectedId] = useState<string | null>(null);
  const [editingInboxId, setEditingInboxId] = useState<string | null>(null);
  const [hoveredInboxId, setHoveredInboxId] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [inboxToDelete, setInboxToDelete] = useState<string | null>(null);

  const {
    data: inboxesPagination,
    isPending,
    isSuccess,
    hasNextPage,
    isFetchingNextPage,
    fetchNextPage,
  } = useGetInboxesWithPagination({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { mutateAsync: removeJob } = useRemoveJob({
    onSuccess: () => {
      toast.success(t('chat.actions.chatsDeleted', { count: 1 }));
      setInboxToDelete(null);
    },
    onError: (error) => {
      toast.error('Failed to delete chat', {
        description: error?.response?.data?.message ?? error.message,
      });
    },
  });

  const { mutateAsync: removeJobs, isPending: isDeleting } = useRemoveJobs({
    onSuccess: (response) => {
      const succeededCount = response.succeeded.length;
      const failedCount = response.failed.length;

      if (response.status === 'success') {
        toast.success(t('chat.actions.chatsDeleted', { count: succeededCount }));
      } else if (response.status === 'partial') {
        toast.warning(
          t('chat.actions.chatsPartiallyDeleted', {
            succeeded: succeededCount,
            failed: failedCount,
          }),
        );
      }

      setSelectedInboxIds(new Set());
      setIsDeleteConfirmOpen(false);
    },
    onError: (error) => {
      toast.error('Failed to delete chats', {
        description: error?.response?.data?.message ?? error.message,
      });
    },
  });

  const allInboxes = useMemo(() => {
    if (!inboxesPagination?.pages) return [];
    return inboxesPagination.pages
      .flatMap((page) => page.inboxes)
      .filter((inbox) => inbox.inbox_id?.startsWith('job_inbox::'));
  }, [inboxesPagination]);

  // Filter inboxes based on search query
  const filteredInboxes = useMemo(() => {
    if (!searchQuery.trim()) return allInboxes;

    const query = searchQuery.toLowerCase();
    return allInboxes.filter((inbox) => {
      const name =
        inbox.last_message && inbox.custom_name === inbox.inbox_id
          ? inbox.last_message.job_message.content
          : inbox.custom_name;
      return name?.toLowerCase().includes(query);
    });
  }, [allInboxes, searchQuery]);

  // Reset selection when dialog closes
  useEffect(() => {
    if (!open) {
      setSearchQuery('');
      setSelectedInboxIds(new Set());
      setIsDeleteConfirmOpen(false);
    }
  }, [open]);

  const handleToggleSelect = useCallback(
    (inboxId: string, shiftKey = false) => {
      setSelectedInboxIds((prev) => {
        const newSet = new Set(prev);
        const isSelecting = !newSet.has(inboxId);

        if (shiftKey && lastSelectedId && isSelecting) {
          const currentIndex = filteredInboxes.findIndex(
            (i) => i.inbox_id === inboxId,
          );
          const lastIndex = filteredInboxes.findIndex(
            (i) => i.inbox_id === lastSelectedId,
          );

          if (currentIndex !== -1 && lastIndex !== -1) {
            const start = Math.min(currentIndex, lastIndex);
            const end = Math.max(currentIndex, lastIndex);

            for (let i = start; i <= end; i++) {
              newSet.add(filteredInboxes[i].inbox_id);
            }
          }
        } else {
          if (newSet.has(inboxId)) {
            newSet.delete(inboxId);
          } else {
            newSet.add(inboxId);
          }
        }
        return newSet;
      });

      setLastSelectedId(inboxId);
    },
    [filteredInboxes, lastSelectedId],
  );

  const handleSelectAll = useCallback(() => {
    const allIds = new Set(filteredInboxes.map((inbox) => inbox.inbox_id));
    setSelectedInboxIds(allIds);
  }, [filteredInboxes]);

  const handleDeselectAll = useCallback(() => {
    setSelectedInboxIds(new Set());
  }, []);

  const handleNavigateToChat = useCallback(
    (inboxId: string) => {
      onOpenChange(false);
      void navigate(`/inboxes/${encodeURIComponent(inboxId)}`);
    },
    [navigate, onOpenChange],
  );

  const handleDelete = useCallback(
    async (inboxId: string) => {
      if (!auth) return;
      await removeJob({
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        jobId: extractJobIdFromInbox(inboxId),
      });
    },
    [auth, removeJob],
  );

  const handleDeleteSelected = useCallback(async () => {
    if (!auth || selectedInboxIds.size === 0) return;

    const jobIds = Array.from(selectedInboxIds).map((inboxId) =>
      extractJobIdFromInbox(inboxId),
    );

    await removeJobs({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      jobIds,
    });
  }, [auth, selectedInboxIds, removeJobs]);

  const allSelected =
    filteredInboxes.length > 0 &&
    filteredInboxes.every((inbox) => selectedInboxIds.has(inbox.inbox_id));

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent showCloseButton className="flex h-[85vh] max-h-[700px] flex-col sm:max-w-[700px]">
        <DialogTitle className="text-lg">
          {t('chat.actions.manageChats')}
        </DialogTitle>
        <DialogDescription>
          {t('chat.actions.manageChatsDescription')}
        </DialogDescription>

        <div className="mt-2 flex items-center gap-2">
          <SearchInput
            classNames={{
              container: 'flex-1',
            }}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t('chat.actions.searchChats')}
            value={searchQuery}
          />
        </div>

        <ScrollArea className="min-h-0 flex-1 pr-4">
          <div
            className={cn(
              'space-y-2 py-2',
              selectedInboxIds.size > 0 && 'pb-20',
            )}
          >
            {isPending &&
              Array.from({ length: 6 }).map((_, index) => (
                <Skeleton
                  className="h-[72px] w-full shrink-0 rounded-lg bg-gray-300"
                  key={index}
                />
              ))}

            {isSuccess && filteredInboxes.length === 0 && (
              <div className="flex flex-col items-center justify-center py-12 text-center">
                <div className="bg-bg-secondary mb-3 flex size-12 items-center justify-center rounded-full">
                  <SearchXIcon className="text-text-tertiary h-6 w-6" />
                </div>
                <p className="text-text-secondary text-sm font-medium">
                  {t('chat.actions.noChatsFound')}
                </p>
                {searchQuery && (
                  <p className="text-text-tertiary mt-1 text-xs">
                    Try adjusting your search terms
                  </p>
                )}
              </div>
            )}

            {isSuccess &&
              filteredInboxes.map((inbox) => {
                const isSelected = selectedInboxIds.has(inbox.inbox_id);
                const isEditing = editingInboxId === inbox.inbox_id;
                const displayName =
                  inbox.last_message && inbox.custom_name === inbox.inbox_id
                    ? inbox.last_message.job_message.content?.slice(0, 80)
                    : inbox.custom_name?.slice(0, 80);

                if (isEditing) {
                  return (
                    <div
                      className="flex items-center gap-4 px-2 py-1.5 bg-bg-secondary"
                      key={inbox.inbox_id}
                    >
                      <InboxNameInput
                        closeEditable={() => setEditingInboxId(null)}
                        inboxId={inbox.inbox_id}
                        inboxName={displayName || inbox.inbox_id}
                      />
                    </div>
                  );
                }

                const isHovered = hoveredInboxId === inbox.inbox_id;

                return (
                  <div
                    className={cn(
                      'flex items-center gap-4 rounded-lg border border-transparent px-4 py-3 transition-colors hover:bg-gray-400/10',
                      isSelected && 'border-brand/30 bg-brand/5',
                    )}
                    key={inbox.inbox_id}
                    onClick={(e) => {
                      const target = e.target as HTMLElement;
                      // Don't toggle selection if clicking on buttons or inputs
                      if (target.closest('button') || target.closest('input')) {
                        return;
                      }
                      handleToggleSelect(inbox.inbox_id, e.shiftKey);
                    }}
                    onMouseEnter={() => setHoveredInboxId(inbox.inbox_id)}
                    onMouseLeave={() => setHoveredInboxId(null)}
                    role="button"
                    tabIndex={0}
                  >
                    <Checkbox
                      checked={isSelected}
                      className="shrink-0"
                      onCheckedChange={() => handleToggleSelect(inbox.inbox_id)}
                      onClick={(e) => e.stopPropagation()}
                    />
                    <div className="flex flex-1 cursor-pointer items-center gap-3 overflow-hidden">
                      <span className="text-text-default truncate text-sm font-medium">
                        {displayName || inbox.inbox_id}
                      </span>
                    </div>

                    {/* Date and Actions container - fixed width to prevent layout shift */}
                    <div className="relative flex h-8 w-[104px] shrink-0 items-center justify-end overflow-hidden">
                      <AnimatePresence initial={false} mode="popLayout">
                        {isHovered ? (
                          <motion.div
                            key="actions"
                            animate={{
                              opacity: 1,
                              x: 0,
                              transition: {
                                duration: 0.2,
                                ease: [0.4, 0, 0.2, 1],
                                staggerChildren: 0.03,
                              }
                            }}
                            className="flex items-center gap-0.5"
                            exit={{
                              opacity: 0,
                              x: 8,
                              transition: { duration: 0.15 }
                            }}
                            initial={{ opacity: 0, x: 8 }}
                          >
                            <motion.div
                              animate={{ opacity: 1, scale: 1 }}
                              initial={{ opacity: 0, scale: 0.8 }}
                              transition={{ duration: 0.15 }}
                            >
                              <Button
                                className="h-8 w-8 p-0"
                                onClick={() => handleNavigateToChat(inbox.inbox_id)}
                                size="auto"
                                title={t('common.open')}
                                variant="tertiary"
                              >
                                <ExternalLinkIcon className="h-4 w-4" />
                              </Button>
                            </motion.div>
                            <motion.div
                              animate={{ opacity: 1, scale: 1 }}
                              initial={{ opacity: 0, scale: 0.8 }}
                              transition={{ duration: 0.15, delay: 0.03 }}
                            >
                              <Button
                                className="h-8 w-8 p-0"
                                onClick={() => setEditingInboxId(inbox.inbox_id)}
                                size="auto"
                                title={t('common.rename')}
                                variant="tertiary"
                              >
                                <Edit3Icon className="h-4 w-4" />
                              </Button>
                            </motion.div>
                            <motion.div
                              animate={{ opacity: 1, scale: 1 }}
                              initial={{ opacity: 0, scale: 0.8 }}
                              transition={{ duration: 0.15, delay: 0.06 }}
                            >
                              <Button
                                className="h-8 w-8 p-0 text-red-500 hover:bg-red-500/10 hover:text-red-500"
                                onClick={() => setInboxToDelete(inbox.inbox_id)}
                                size="auto"
                                title={t('common.delete')}
                                variant="tertiary"
                              >
                                <Trash2Icon className="h-4 w-4" />
                              </Button>
                            </motion.div>
                          </motion.div>
                        ) : (
                          <motion.span
                            key="date"
                            animate={{
                              opacity: 1,
                              x: 0,
                              transition: {
                                duration: 0.2,
                                ease: [0.4, 0, 0.2, 1]
                              }
                            }}
                            className="text-text-tertiary whitespace-nowrap text-xs"
                            exit={{
                              opacity: 0,
                              x: -8,
                              transition: { duration: 0.15 }
                            }}
                            initial={{ opacity: 0, x: -8 }}
                          >
                            {formatDistanceToNow(
                              new Date(inbox.datetime_created),
                              { addSuffix: true },
                            )}
                          </motion.span>
                        )}
                      </AnimatePresence>
                    </div>
                  </div>
                );
              })}

            {hasNextPage && (
              <button
                className="text-text-secondary hover:text-text-default mx-auto mt-4 block w-full py-3 text-center text-sm"
                disabled={isFetchingNextPage}
                onClick={() => fetchNextPage()}
                type="button"
              >
                {isFetchingNextPage ? 'Loading more...' : 'Load more'}
              </button>
            )}
          </div>
        </ScrollArea>

        {selectedInboxIds.size > 0 && (
          <div className="border-divider bg-bg-default absolute right-0 bottom-0 left-0 flex items-center justify-between gap-3 rounded-b-lg border-t px-6 py-4">
            <span className="text-text-secondary text-sm font-medium">
              {t('chat.actions.selectedCount', {
                count: selectedInboxIds.size,
              })}
            </span>
            <div className="flex items-center gap-2">
              <Button
                onClick={allSelected ? handleDeselectAll : handleSelectAll}
                size="sm"
                variant="outline"
              >
                {allSelected
                  ? t('chat.actions.deselectAll')
                  : t('chat.actions.selectAll')}
              </Button>
              <Button
                onClick={() => setIsDeleteConfirmOpen(true)}
                size="sm"
                variant="destructive"
              >
                <Trash2Icon className="mr-1.5 h-4 w-4" />
                {t('chat.actions.deleteSelectedChats')}
              </Button>
            </div>
          </div>
        )}

        {/* Delete Confirmation Dialog */}
        <Dialog onOpenChange={setIsDeleteConfirmOpen} open={isDeleteConfirmOpen}>
          <DialogContent showCloseButton className="sm:max-w-[425px]">
            <DialogTitle>
              {t('chat.actions.deleteSelectedChatsConfirmationTitle', {
                count: selectedInboxIds.size,
              })}
            </DialogTitle>
            <DialogDescription>
              {t('chat.actions.deleteSelectedChatsConfirmationDescription')}
            </DialogDescription>

            <DialogFooter>
              <div className="flex gap-2 pt-4">
                <Button
                  className="min-w-[100px] flex-1"
                  disabled={isDeleting}
                  onClick={() => setIsDeleteConfirmOpen(false)}
                  size="sm"
                  variant="outline"
                >
                  {t('common.cancel')}
                </Button>
                <Button
                  className="min-w-[100px] flex-1"
                  disabled={isDeleting}
                  isLoading={isDeleting}
                  onClick={handleDeleteSelected}
                  size="sm"
                  variant="destructive"
                >
                  {t('common.delete')}
                </Button>
              </div>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* Single Delete Confirmation Dialog */}
        <Dialog
          onOpenChange={(open) => !open && setInboxToDelete(null)}
          open={!!inboxToDelete}
        >
          <DialogContent showCloseButton className="sm:max-w-[425px]">
            <DialogTitle>
              {t('chat.actions.deleteInboxConfirmationTitle')}
            </DialogTitle>
            <DialogDescription>
              {t('chat.actions.deleteInboxConfirmationDescription')}
            </DialogDescription>

            <DialogFooter>
              <div className="flex gap-2 pt-4">
                <Button
                  className="min-w-[100px] flex-1"
                  onClick={() => setInboxToDelete(null)}
                  size="sm"
                  variant="outline"
                >
                  {t('common.cancel')}
                </Button>
                <Button
                  className="min-w-[100px] flex-1"
                  onClick={() => inboxToDelete && handleDelete(inboxToDelete)}
                  size="sm"
                  variant="destructive"
                >
                  {t('common.delete')}
                </Button>
              </div>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </DialogContent>
    </Dialog>
  );
}
