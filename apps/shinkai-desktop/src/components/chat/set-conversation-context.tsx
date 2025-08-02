import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { retrieveVectorResource } from '@shinkai_network/shinkai-message-ts/api/vector-fs/index';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import {
  type SearchVectorFormSchema,
  searchVectorFormSchema,
} from '@shinkai_network/shinkai-node-state/forms/vector-fs/vector-search';
import { transformDataToTreeNodes } from '@shinkai_network/shinkai-node-state/lib/utils/files';
import { useUpdateJobScope } from '@shinkai_network/shinkai-node-state/v2/mutations/updateJobScope/useUpdateJobScope';
import { useGetListDirectoryContents } from '@shinkai_network/shinkai-node-state/v2/queries/getDirectoryContents/useGetListDirectoryContents';
import { useGetJobFolderName } from '@shinkai_network/shinkai-node-state/v2/queries/getJobFolderName/useGetJobFolderName';
import { useGetVRSeachSimplified } from '@shinkai_network/shinkai-node-state/v2/queries/getVRSearchSimplified/useGetSearchVRItems';
import {
  Badge,
  Button,
  Form,
  FormField,
  ScrollArea,
  SearchInput,
  Sheet,
  SheetContent,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from '@shinkai_network/shinkai-ui';
import { FileTypeIcon } from '@shinkai_network/shinkai-ui/assets';
import { SearchIcon } from 'lucide-react';
import { Checkbox } from 'primereact/checkbox';
import { Tree, type TreeCheckboxSelectionKeys } from 'primereact/tree';
import { type TreeNode } from 'primereact/treenode';
import { useEffect, useState } from 'react';
import { useForm, useWatch } from 'react-hook-form';
import { useParams } from 'react-router';
import { toast } from 'sonner';

import { treeOptions } from '../../lib/constants';
import { useAuth } from '../../store/auth';
import { useSetJobScope } from './context/set-job-scope-context';

export const SetJobScopeDrawer = () => {
  const { t } = useTranslation();
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = decodeURIComponent(encodedInboxId);

  const isSetJobScopeOpen = useSetJobScope((state) => state.isSetJobScopeOpen);
  const setSetJobScopeOpen = useSetJobScope(
    (state) => state.setSetJobScopeOpen,
  );
  const selectedKeys = useSetJobScope((state) => state.selectedKeys);
  const onSelectedKeysChange = useSetJobScope(
    (state) => state.onSelectedKeysChange,
  );

  const selectedFileKeysRef = useSetJobScope(
    (state) => state.selectedFileKeysRef,
  );
  const selectedFolderKeysRef = useSetJobScope(
    (state) => state.selectedFolderKeysRef,
  );
  const auth = useAuth((state) => state.auth);
  const [nodes, setNodes] = useState<TreeNode[]>([]);

  // Helper function to find a TreeNode by its key (path) in the tree
  const findNodeByKey = (
    key: string,
    searchNodes: TreeNode[],
  ): TreeNode | null => {
    for (const node of searchNodes) {
      if (String(node.key) === key) {
        return node;
      }
      if (node.children) {
        const found = findNodeByKey(key, node.children);
        if (found) {
          return found;
        }
      }
    }
    return null;
  };

  const { data: fileInfoArray, isSuccess: isVRFilesSuccess } =
    useGetListDirectoryContents({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      path: '/',
      depth: 6,
    });

  const { mutateAsync: updateJobScope, isPending: isUpdatingJobScope } =
    useUpdateJobScope({
      onSuccess: () => {
        setSetJobScopeOpen(false);
      },
      onError: (error) => {
        toast.error('Failed to update conversation context', {
          description: error.response?.data?.message ?? error.message,
        });
      },
    });

  const { data: jobFolderData } = useGetJobFolderName(
    {
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    },
    {
      enabled: !!inboxId,
    },
  );

  const selectedPaths = jobFolderData ? [jobFolderData.folder_name] : [];

  useEffect(() => {
    if (isVRFilesSuccess) {
      setNodes(
        transformDataToTreeNodes(fileInfoArray, undefined, selectedPaths),
      );
    }
  }, [fileInfoArray, isVRFilesSuccess, jobFolderData]);

  useEffect(() => {
    if (!isSetJobScopeOpen) {
      const element = document.querySelector('#chat-input') as HTMLDivElement;
      if (element) {
        element?.focus?.();
      }
    }
  }, [isSetJobScopeOpen]);

  return (
    <Sheet onOpenChange={setSetJobScopeOpen} open={isSetJobScopeOpen}>
      <SheetContent>
        <SheetHeader className="mb-3">
          <SheetTitle className="flex h-[40px] items-center gap-4">
            {t('chat.form.setContext')}
            {Object.keys(selectedKeys ?? {}).length > 0 && (
              <Badge className="bg-brand text-sm text-white">
                {Object.keys(selectedKeys ?? {}).length}
              </Badge>
            )}
          </SheetTitle>
          <p className="text-text-secondary text-sm">
            {t('chat.form.setContextText')}
          </p>
        </SheetHeader>
        <ScrollArea className="h-[calc(100vh-200px)] flex-1">
          <Tree
            onSelect={(e) => {
              if (e.node.icon === 'icon-folder') {
                selectedFolderKeysRef.set(String(e.node.key), e.node.data.path);
                return;
              }
              selectedFileKeysRef.set(String(e.node.key), e.node.data.path);
            }}
            onSelectionChange={(e) => {
              onSelectedKeysChange(e.value as TreeCheckboxSelectionKeys);
            }}
            onUnselect={(e) => {
              const nodeKey = String(e.node.key);

              if (e.node.icon === 'icon-folder') {
                // --- Folder Deselected ---
                selectedFolderKeysRef.delete(nodeKey);

                // Robustly remove all descendant keys (files and folders) from both refs
                const clearDescendants = (node: TreeNode) => {
                  node.children?.forEach((child) => {
                    const childKey = String(child.key);
                    selectedFileKeysRef.delete(childKey); // Remove if it exists as a file key
                    selectedFolderKeysRef.delete(childKey); // Remove if it exists as a folder key
                    if (child.children && child.children.length > 0) {
                      clearDescendants(child);
                    }
                  });
                };
                clearDescendants(e.node);
              } else {
                // --- File Deselected ---
                const lastSlashIndex = nodeKey.lastIndexOf('/');
                const parentFolderPath =
                  lastSlashIndex > 0
                    ? nodeKey.substring(0, lastSlashIndex)
                    : '/';
                const isParentFolderSelected =
                  selectedFolderKeysRef.has(parentFolderPath);

                if (isParentFolderSelected) {
                  // Parent folder *was* selected. Transition to selecting individual siblings.

                  // 1. Remove the parent folder from selection.
                  selectedFolderKeysRef.delete(parentFolderPath);

                  // 2. Find the parent node to access its children.
                  const parentNode = findNodeByKey(parentFolderPath, nodes);

                  // 3. Add all *other* sibling *files* to the individual file selection.
                  (parentNode?.children ?? [])
                    .filter(
                      (childNode: TreeNode) =>
                        String(childNode.key) !== nodeKey &&
                        childNode.data?.path,
                    ) // Only other files
                    .forEach((childNode: TreeNode) => {
                      if (childNode.icon !== 'icon-folder') {
                        // Ensure path exists and is a file
                        selectedFileKeysRef.set(
                          String(childNode.key),
                          childNode.data.path,
                        );
                      } else {
                        selectedFolderKeysRef.set(
                          String(childNode.key),
                          childNode.data.path,
                        );
                      }
                    });
                } else {
                  // Parent folder was NOT selected, so this file was individually selected.
                  selectedFileKeysRef.delete(nodeKey);
                }
              }
            }}
            propagateSelectionDown={true}
            propagateSelectionUp={true}
            pt={treeOptions}
            selectionKeys={selectedKeys}
            selectionMode="checkbox"
            value={nodes}
          />
        </ScrollArea>

        <SheetFooter className="flex-row items-center gap-3">
          <Button
            className="flex-1"
            onClick={() => {
              onSelectedKeysChange(null);
              selectedFileKeysRef.clear();
              selectedFolderKeysRef.clear();
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            {t('common.unselectAll')}
          </Button>
          <Button
            className="flex-1"
            isLoading={isUpdatingJobScope}
            onClick={async () => {
              if (inboxId) {
                await updateJobScope({
                  jobId: extractJobIdFromInbox(inboxId),
                  nodeAddress: auth?.node_address ?? '',
                  token: auth?.api_v2_key ?? '',
                  jobScope: {
                    vector_fs_items: Array.from(selectedFileKeysRef.values()),
                    vector_fs_folders: Array.from(
                      selectedFolderKeysRef.values(),
                    ),
                  },
                });
              }
              setSetJobScopeOpen(false);
            }}
            size="sm"
            type="button"
          >
            {inboxId ? t('common.saveChanges') : t('common.done')}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
};

export const KnowledgeSearchDrawer = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const isKnowledgeSearchOpen = useSetJobScope(
    (state) => state.isKnowledgeSearchOpen,
  );
  const setKnowledgeSearchOpen = useSetJobScope(
    (state) => state.setKnowledgeSearchOpen,
  );

  const selectedKeys = useSetJobScope((state) => state.selectedKeys);
  const onSelectedKeysChange = useSetJobScope(
    (state) => state.onSelectedKeysChange,
  );
  const selectedFileKeysRef = useSetJobScope(
    (state) => state.selectedFileKeysRef,
  );

  const [isSearchEntered, setIsSearchEntered] = useState(false);
  const [search, setSearch] = useState('');
  const searchVectorFSForm = useForm<SearchVectorFormSchema>({
    resolver: zodResolver(searchVectorFormSchema),
    defaultValues: {
      searchQuery: '',
    },
  });
  const currentSearchQuery = useWatch({
    control: searchVectorFSForm.control,
    name: 'searchQuery',
  });

  const { isPending, isLoading, isSuccess, data } = useGetVRSeachSimplified(
    {
      nodeAddress: auth?.node_address ?? '',
      search: search,
      token: auth?.api_v2_key ?? '',
    },
    {
      enabled: isSearchEntered || !!search,
      refetchOnWindowFocus: false,
    },
  );

  const onSubmit = async (data: SearchVectorFormSchema) => {
    if (!data.searchQuery) return;
    setIsSearchEntered(true);
    setSearch(data.searchQuery);
  };

  const groupedData = data?.reduce<Record<string, string[]>>(
    (acc, [content, pathList]) => {
      const generatedFilePath = '/' + pathList.join('/');
      if (!acc[generatedFilePath]) {
        acc[generatedFilePath] = [];
      }
      acc[generatedFilePath].push(content);
      return acc;
    },
    {},
  );

  return (
    <Sheet onOpenChange={setKnowledgeSearchOpen} open={isKnowledgeSearchOpen}>
      <SheetContent>
        <SheetHeader className="mb-3">
          <SheetTitle className="flex h-[40px] items-center gap-4">
            {t('aiFilesSearch.label')}
          </SheetTitle>
          <p className="text-text-secondary text-sm">
            {t('aiFilesSearch.description')}
          </p>
        </SheetHeader>

        <Form {...searchVectorFSForm}>
          <form
            className="flex shrink-0 flex-col items-center gap-2 pt-4"
            onSubmit={searchVectorFSForm.handleSubmit(onSubmit)}
          >
            <div className="flex w-full flex-1 items-center gap-2">
              <FormField
                control={searchVectorFSForm.control}
                name="searchQuery"
                render={({ field }) => (
                  <SearchInput
                    classNames={{
                      container: 'mx-0.5 h-9',
                      input: 'bg-transparent',
                    }}
                    onChange={field.onChange}
                    value={field.value}
                  />
                )}
              />
              <Button
                className="h-[48px] w-[48px] shrink-0 rounded-xl p-3.5"
                disabled={isPending && isLoading}
                isLoading={isPending && isLoading}
                size="auto"
                type="submit"
              >
                <SearchIcon />
                <span className="sr-only">{t('common.search')}</span>
              </Button>
            </div>
          </form>
        </Form>
        <ScrollArea className="h-[calc(100vh-340px)] pr-4 [&>div>div]:!block">
          {isSearchEntered &&
            isPending &&
            Array.from({ length: 4 }).map((_, idx) => (
              <div
                className="mb-1 flex h-[69px] items-center justify-between gap-2 rounded-lg bg-gray-400 py-3"
                key={idx}
              />
            ))}
          {isSearchEntered && isSuccess && (
            <div>
              <div className="flex items-center justify-between gap-4 p-2">
                <h2 className="text-text-secondary text-sm font-medium">
                  {t('aiFilesSearch.foundResults', {
                    count: data?.length,
                  })}
                </h2>
                {selectedKeys && Object.keys(selectedKeys).length > 0 && (
                  <span className="text-brand text-sm font-medium">
                    {t('aiFilesSearch.filesSelected', {
                      count: Object.keys(selectedKeys).length,
                    })}
                  </span>
                )}
              </div>
              <div className="flex flex-col gap-2">
                {Object.entries(groupedData ?? {}).map(
                  ([generatedFilePath, contents]) => (
                    <div
                      className="flex items-start gap-1 px-2 py-3 text-sm"
                      key={generatedFilePath}
                    >
                      <Checkbox
                        checked={generatedFilePath in (selectedKeys || {})}
                        inputId={generatedFilePath}
                        name="files"
                        onChange={async (event) => {
                          const newKeys = { ...selectedKeys };
                          if (event.value in (selectedKeys || {})) {
                            delete newKeys[event.value];
                          } else {
                            newKeys[event.value] = { checked: true };
                            const fileInfo = await retrieveVectorResource(
                              auth?.node_address ?? '',
                              auth?.api_v2_key ?? '',
                              { path: generatedFilePath },
                            );

                            selectedFileKeysRef.set(event.value, {
                              ...fileInfo.data,
                              path: event.value,
                              vr_header: {
                                resource_name: fileInfo.data.name,
                                resource_source: fileInfo.data.source,
                              },
                            });
                          }
                          onSelectedKeysChange(newKeys);
                        }}
                        value={generatedFilePath}
                      />
                      <label
                        className="ml-2 flex-1"
                        htmlFor={generatedFilePath}
                      >
                        <div className="flex items-center gap-1">
                          <FileTypeIcon className="h-6 w-6" />
                          <span className="text-sm">
                            {generatedFilePath.split('/').at(-1)}
                          </span>
                        </div>
                        <div className="divide-y divide-gray-300">
                          {contents?.map((content) => (
                            <p
                              className="text-text-secondary py-3 text-xs"
                              key={content}
                            >
                              {content}
                            </p>
                          ))}
                        </div>
                      </label>
                    </div>
                  ),
                )}
              </div>
            </div>
          )}
        </ScrollArea>
        <SheetFooter>
          <Button
            onClick={() => {
              onSelectedKeysChange(null);
              selectedFileKeysRef.clear();
            }}
            type="button"
            variant="outline"
          >
            {t('common.unselectAll')}
          </Button>
          <Button
            onClick={() => {
              setKnowledgeSearchOpen(false);
            }}
            type="button"
          >
            {t('common.done')}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
};
