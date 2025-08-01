import { HomeIcon } from '@radix-ui/react-icons';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useGetListDirectoryContents } from '@shinkai_network/shinkai-node-state/v2/queries/getDirectoryContents/useGetListDirectoryContents';
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
  ScrollArea,
  Skeleton,
} from '@shinkai_network/shinkai-ui';
import { DirectoryTypeIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { ChevronRight } from 'lucide-react';
import React, { createContext, useContext, useState } from 'react';
import { createStore, useStore } from 'zustand';

import { useAuth } from '../../../store/auth';
import { VectorFsFolderInfo } from './vector-fs-folder';

type VectorFolderSelectionList = {
  destinationFolderPath: string | null;
  setDestinationFolderPath: (folder: string | null) => void;
  currentSelectedFolderPath: string;
  setCurrentSelectedFolderPath: (path: string) => void;
};

const createVectorFsStore = () =>
  createStore<VectorFolderSelectionList>((set) => ({
    destinationFolderPath: null,
    setDestinationFolderPath: (destinationFolderPath) => {
      set({ destinationFolderPath });
    },
    currentSelectedFolderPath: '/',
    setCurrentSelectedFolderPath: (currentSelectedFolderPath) => {
      set({ currentSelectedFolderPath });
    },
  }));

const VectorFolderSelectionContext = createContext<ReturnType<
  typeof createVectorFsStore
> | null>(null);

export const VectorFolderSelectionProvider = ({
  children,
}: {
  children: React.ReactNode;
}) => {
  const [store] = useState<ReturnType<typeof createVectorFsStore>>(
    createVectorFsStore(),
  );

  return (
    <VectorFolderSelectionContext.Provider value={store}>
      {children}
    </VectorFolderSelectionContext.Provider>
  );
};

export function useVectorFolderSelectionStore<T>(
  selector: (state: VectorFolderSelectionList) => T,
) {
  const store = useContext(VectorFolderSelectionContext);
  if (!store) {
    throw new Error('Missing VectorFolderSelectionProvider');
  }
  const value = useStore(store, selector);
  return value;
}

export const FolderSelectionList = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const destinationFolderPath = useVectorFolderSelectionStore(
    (state) => state.destinationFolderPath,
  );
  const setDestinationFolderPath = useVectorFolderSelectionStore(
    (state) => state.setDestinationFolderPath,
  );
  const currentSelectedFolderPath = useVectorFolderSelectionStore(
    (state) => state.currentSelectedFolderPath,
  );
  const setCurrentSelectedFolderPath = useVectorFolderSelectionStore(
    (state) => state.setCurrentSelectedFolderPath,
  );

  const { isPending: isVRFilesPending, data: VRFiles } =
    useGetListDirectoryContents({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      path: currentSelectedFolderPath,
    });

  const splitCurrentPath =
    destinationFolderPath?.split('/').filter(Boolean) ?? [];

  return (
    <div className="space-y-2 py-6">
      <Breadcrumb>
        <BreadcrumbList>
          <BreadcrumbItem>
            <BreadcrumbLink asChild>
              <button
                className={cn(
                  'hover:bg-bg-secondary flex items-center gap-2 rounded-full p-2',
                  currentSelectedFolderPath === '/' && 'text-white',
                )}
                onClick={() => {
                  setCurrentSelectedFolderPath('/');
                  setDestinationFolderPath('/');
                }}
                type="button"
              >
                <HomeIcon className="h-3.5 w-3.5" />
                {t('vectorFs.home')}
              </button>
            </BreadcrumbLink>
          </BreadcrumbItem>
          {splitCurrentPath.map((path, idx) => (
            <React.Fragment key={idx}>
              <BreadcrumbSeparator>
                <ChevronRight />
              </BreadcrumbSeparator>
              {idx === splitCurrentPath.length - 1 ? (
                <BreadcrumbPage className="flex items-center gap-1 p-2 font-medium">
                  {destinationFolderPath?.split('/')?.at(-1)}
                </BreadcrumbPage>
              ) : (
                <BreadcrumbLink asChild>
                  <button
                    className="hover:bg-bg-secondary flex items-center gap-1 rounded-full bg-transparent p-2"
                    onClick={() => {
                      const buildPath = splitCurrentPath
                        .slice(0, idx + 1)
                        .join('/');

                      setCurrentSelectedFolderPath('/' + buildPath);
                      setDestinationFolderPath(buildPath);
                    }}
                    type="button"
                  >
                    {path}
                  </button>
                </BreadcrumbLink>
              )}
            </React.Fragment>
          ))}
        </BreadcrumbList>
      </Breadcrumb>
      <ScrollArea className="min-h-[300px]">
        <div
          className={cn('divide-divider grid flex-1 grid-cols-1 divide-y py-1')}
        >
          {isVRFilesPending &&
            Array.from({ length: 4 }).map((_, idx) => (
              <Skeleton
                className="mb-1 flex h-[69px] items-center justify-between gap-2 rounded-lg py-3"
                key={idx}
              />
            ))}
          {VRFiles?.map((folder) => {
            return (
              <button
                className={cn(
                  'hover:bg-bg-secondary flex items-center justify-between gap-2 p-2 py-3.5',
                  destinationFolderPath === folder.path && 'bg-bg-secondary',
                )}
                key={folder.path}
                onClick={() => {
                  if ((folder.children ?? []).length > 0) {
                    setCurrentSelectedFolderPath(folder.path);
                    setDestinationFolderPath(folder.path);
                  } else {
                    setDestinationFolderPath(folder.path);
                  }
                }}
                type="button"
              >
                <DirectoryTypeIcon />
                <VectorFsFolderInfo allowFolderNameOnly folder={folder} />
                {!!(folder?.children ?? []).length && (
                  <ChevronRight className="text-text-secondary h-5 w-5" />
                )}
              </button>
            );
          })}
        </div>
      </ScrollArea>
    </div>
  );
};
