import { HomeIcon } from '@radix-ui/react-icons';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type DirectoryContent } from '@shinkai_network/shinkai-message-ts/api/vector-fs/types';
import { useGetListDirectoryContents } from '@shinkai_network/shinkai-node-state/v2/queries/getDirectoryContents/useGetListDirectoryContents';
import { useGetSearchDirectoryContents } from '@shinkai_network/shinkai-node-state/v2/queries/getSearchDirectoryContents/useGetSearchDirectoryContents';
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
  Button,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  ScrollArea,
  SearchInput,
  Separator,
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import {
  AddNewFolderIcon,
  CreateAIIcon,
  FileEmptyStateIcon,
  FileTypeIcon,
  GenerateDocIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { motion } from 'framer-motion';
import { ChevronRight, FileType2Icon, PlusIcon, XIcon } from 'lucide-react';
import React, { useEffect } from 'react';
import { useLocation, useNavigate } from 'react-router';

import { useDebounce } from '../../../hooks/use-debounce';
import { useURLQueryParams } from '../../../hooks/use-url-query-params';
import { useAuth } from '../../../store/auth';
import { useVectorFsStore, VectorFSLayout } from '../context/vector-fs-context';
import { VectorFsGlobalAction } from './vector-fs-drawer';
import VectorFsFolder from './vector-fs-folder';
import VectorFsItem from './vector-fs-item';
import VectorFsToggleLayout from './vector-fs-toggle-layout';
import VectorFsToggleSortName from './vector-fs-toggle-sort-name';

const MotionButton = motion(Button);

const AllFiles = () => {
  const auth = useAuth((state) => state.auth);
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const query = useURLQueryParams();

  const currentGlobalPath = useVectorFsStore(
    (state) => state.currentGlobalPath,
  );

  const setCurrentGlobalPath = useVectorFsStore(
    (state) => state.setCurrentGlobalPath,
  );
  const setActiveDrawerMenuOption = useVectorFsStore(
    (state) => state.setActiveDrawerMenuOption,
  );
  const layout = useVectorFsStore((state) => state.layout);
  const isSortByName = useVectorFsStore((state) => state.isSortByName);

  const isVRSelectionActive = useVectorFsStore(
    (state) => state.isVRSelectionActive,
  );
  const setVRSelectionActive = useVectorFsStore(
    (state) => state.setVRSelectionActive,
  );
  const [searchQuery, setSearchQuery] = React.useState('');
  const debouncedSearchQuery = useDebounce(searchQuery, 600);
  const {
    isPending: isVRFilesPending,
    data: fileInfoArray,
    isSuccess: isVRFilesSuccess,
  } = useGetListDirectoryContents(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      path: currentGlobalPath,
    },
    {
      select: (data) => {
        return data?.sort((a, b) => a.name.localeCompare(b.name));
      },
      refetchInterval: 6000,
    },
  );

  const {
    data: searchVRItems,
    isSuccess: isSearchVRItemsSuccess,
    isLoading: isSearchVRItemsLoading,
  } = useGetSearchDirectoryContents(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      // path: currentGlobalPath,
      name: debouncedSearchQuery,
    },
    {
      enabled: !!debouncedSearchQuery,
    },
  );

  const setSelectedFile = useVectorFsStore((state) => state.setSelectedFile);
  const [selectedFiles, setSelectedFiles] = React.useState<DirectoryContent[]>(
    [],
  );
  const [selectedFolders, setSelectedFolders] = React.useState<
    DirectoryContent[]
  >([]);
  const [isMenuOpened, setMenuOpened] = React.useState(false);

  useEffect(() => {
    const isFileDetailsActive = query.has('path');
    if (isFileDetailsActive) {
      const path = decodeURIComponent(query.get('path') ?? '');
      const directoryMainPath = path.split('/').slice(0, -1);
      setCurrentGlobalPath(
        directoryMainPath.length > 1
          ? '/' + directoryMainPath.join('/')
          : '/' + directoryMainPath,
      );
    }
  }, [query]);
  const handleSelectFiles = (file: DirectoryContent) => {
    if (selectedFiles.some((selectedFile) => selectedFile.path === file.path)) {
      setSelectedFiles(selectedFiles.filter((item) => item !== file));
    } else {
      setSelectedFiles([...selectedFiles, file]);
    }
  };

  const handleSelectFolders = (folder: DirectoryContent) => {
    if (
      selectedFolders.some(
        (selectedFolder) => selectedFolder.path === folder.path,
      )
    ) {
      setSelectedFolders(selectedFolders.filter((item) => item !== folder));
    } else {
      setSelectedFolders([...selectedFolders, folder]);
    }
  };

  useEffect(() => {
    if (location.state?.files) {
      setActiveDrawerMenuOption(
        VectorFsGlobalAction.GenerateFromDocumentIncludeFolder,
      );
    }
  }, [location.state?.files, setActiveDrawerMenuOption]);

  const actionList = [
    {
      name: t('vectorFs.actions.createFolder'),
      icon: <AddNewFolderIcon className="mr-2 h-4 w-4" />,
      onClick: () => {
        setActiveDrawerMenuOption(VectorFsGlobalAction.NewFolder);
      },
    },
    {
      name: t('vectorFs.actions.uploadFile'),
      icon: <GenerateDocIcon className="mr-2 h-4 w-4" />,
      onClick: () => {
        setActiveDrawerMenuOption(VectorFsGlobalAction.GenerateFromDocument);
      },
    },

    {
      name: t('vectorFs.actions.createTextFile'),
      icon: <FileType2Icon className="mr-2 h-3.5 w-3.5" />,
      onClick: () => {
        setActiveDrawerMenuOption(VectorFsGlobalAction.CreateTextFile);
      },
    },
  ];

  const splitCurrentPath = currentGlobalPath.split('/').filter(Boolean);

  const folderList = React.useMemo(() => {
    const folders = fileInfoArray?.filter((file) => file.is_directory) ?? [];
    return isSortByName ? folders : [...folders].reverse();
  }, [fileInfoArray, isSortByName]);

  const itemList = React.useMemo(() => {
    const items = fileInfoArray?.filter((file) => !file.is_directory) ?? [];
    return isSortByName ? items : [...items].reverse();
  }, [fileInfoArray, isSortByName]);

  return (
    <div className="relative flex h-full flex-col">
      <DropdownMenu
        modal={false}
        onOpenChange={(value) => setMenuOpened(value)}
        open={isMenuOpened}
      >
        <DropdownMenuTrigger asChild>
          <Button
            className="absolute -top-14 right-0 flex gap-2 self-end"
            size="sm"
          >
            <PlusIcon className="h-4 w-4" /> {t('vectorFs.actions.addNew')}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="end"
          alignOffset={-10}
          className="w-[200px] px-2 py-2"
          sideOffset={10}
        >
          {actionList.map((item, index) => (
            <DropdownMenuItem key={index} onClick={item.onClick}>
              {item.icon}
              <span>{item.name}</span>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <div className="mt-2 flex justify-between gap-3">
        <SearchInput
          classNames={{ container: 'max-w-[500px]', input: 'bg-transparent' }}
          onChange={(e) => {
            setSearchQuery(e.target.value);
          }}
          value={searchQuery}
        />

        <div className="flex gap-3">
          <VectorFsToggleSortName />
          <Separator className="bg-divider" decorative orientation="vertical" />
          <VectorFsToggleLayout />
        </div>
      </div>
      {!searchQuery && (
        <div className="mt-4">
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>
                <BreadcrumbLink asChild>
                  <Button
                    className={cn(
                      currentGlobalPath === '/' && 'text-text-default',
                    )}
                    onClick={() => {
                      setCurrentGlobalPath('/');
                    }}
                    size="sm"
                    variant="tertiary"
                  >
                    <HomeIcon className="h-3.5 w-3.5" />
                    {t('vectorFs.home')}
                  </Button>
                </BreadcrumbLink>
              </BreadcrumbItem>
              {splitCurrentPath.map((path, idx) => (
                <React.Fragment key={idx}>
                  <BreadcrumbSeparator>
                    <ChevronRight />
                  </BreadcrumbSeparator>
                  {splitCurrentPath.length - 1 === idx ? (
                    <BreadcrumbPage className="flex items-center gap-1 p-2 font-medium">
                      {path}
                    </BreadcrumbPage>
                  ) : (
                    <BreadcrumbLink asChild>
                      <Button
                        onClick={() => {
                          const buildPath = splitCurrentPath
                            .slice(0, idx + 1)
                            .join('/');
                          setCurrentGlobalPath('/' + buildPath);
                        }}
                        size="sm"
                        variant="tertiary"
                      >
                        {path}
                      </Button>
                    </BreadcrumbLink>
                  )}
                </React.Fragment>
              ))}
            </BreadcrumbList>
          </Breadcrumb>
        </div>
      )}
      <ScrollArea>
        <div
          className={cn(
            'grid flex-1',
            layout === VectorFSLayout.Grid &&
              'grid-cols-2 gap-3 md:grid-cols-3',
            layout === VectorFSLayout.List &&
              'divide-divider grid-cols-1 divide-y',
            searchQuery && 'pt-4',
          )}
        >
          {(isVRFilesPending ||
            isSearchVRItemsLoading ||
            (searchQuery && searchQuery !== debouncedSearchQuery)) &&
            Array.from({ length: 4 }).map((_, idx) => (
              <Skeleton
                className="mb-2 flex h-[69px] items-center justify-between gap-2 rounded-lg py-3"
                key={idx}
              />
            ))}
          {!searchQuery &&
            folderList?.map((folder, index: number) => {
              return (
                <VectorFsFolder
                  folder={folder}
                  handleSelectFolders={handleSelectFolders}
                  isSelectedFolder={selectedFolders.some(
                    (selectedFolder) => selectedFolder.path === folder.path,
                  )}
                  key={index}
                  onClick={() => {
                    setCurrentGlobalPath(folder.path);
                  }}
                />
              );
            })}
          {!searchQuery &&
            isVRFilesSuccess &&
            itemList?.map((file, index: number) => {
              return (
                <VectorFsItem
                  file={file}
                  handleSelectFiles={handleSelectFiles}
                  isSelectedFile={selectedFiles.some(
                    (selectedFile) => selectedFile.path === file.path,
                  )}
                  key={index}
                  onClick={() => {
                    setSelectedFile(file);
                    setActiveDrawerMenuOption(
                      VectorFsGlobalAction.VectorFileDetails,
                    );
                  }}
                />
              );
            })}
          {!searchQuery &&
            isVRFilesSuccess &&
            folderList.length === 0 &&
            currentGlobalPath === '/' && (
              <div className="text-text-secondary mt-4 flex flex-col items-center justify-center gap-4 text-center text-base">
                <FileEmptyStateIcon className="h-20 w-20" />
                <div>
                  <h2 className="font-medium text-white">
                    {t('vectorFs.emptyState.noFilesAndFolders')}
                  </h2>
                  <span>
                    {t('vectorFs.emptyState.noFilesAndFoldersDescription')}
                  </span>
                </div>
              </div>
            )}
          {!searchQuery &&
            isVRFilesSuccess &&
            itemList.length === 0 &&
            folderList.length === 0 && (
              <div className="text-text-secondary flex h-20 items-center justify-center">
                {t('vectorFs.emptyState.noFiles')}
              </div>
            )}
          {searchQuery &&
            isSearchVRItemsSuccess &&
            searchVRItems?.length === 0 && (
              <div className="text-text-secondary flex h-20 items-center justify-center">
                {t('vectorFs.emptyState.noFiles')}
              </div>
            )}
          {searchQuery &&
            isSearchVRItemsSuccess &&
            searchQuery === debouncedSearchQuery &&
            searchVRItems?.map((item) => {
              return (
                <button
                  className="hover:to-bg-quaternary relative flex items-center gap-2 px-3 py-1.5 text-ellipsis hover:bg-gradient-to-r hover:from-transparent"
                  key={item.path}
                  onClick={() => {
                    const directoryMainPath = item.path.split('/').slice(0, -1);
                    setCurrentGlobalPath(directoryMainPath.join('/'));
                    setSearchQuery('');
                  }}
                >
                  <FileTypeIcon />
                  <span className="text-text-default text-sm">
                    {item?.name}
                  </span>
                </button>
              );
            })}
        </div>
      </ScrollArea>
      <ContextMenu modal={false}>
        <ContextMenuTrigger className="flex-1">
          <span className="sr-only">{t('common.seeOptions')}</span>
        </ContextMenuTrigger>
        <ContextMenuContent className="p-2">
          {actionList.map((item, index) => (
            <ContextMenuItem key={index} onClick={item.onClick}>
              {item.icon}
              <span>{item.name}</span>
            </ContextMenuItem>
          ))}
        </ContextMenuContent>
      </ContextMenu>

      <TooltipProvider delayDuration={0}>
        <Tooltip>
          <TooltipTrigger asChild>
            <MotionButton
              className={cn(
                'fixed right-8 bottom-8 h-[60px] w-[60px]',
                isVRSelectionActive && 'w-[210px]',
              )}
              layout
              onClick={() => {
                if (!isVRSelectionActive) {
                  setVRSelectionActive(true);
                } else {
                  void navigate('/home', {
                    state: {
                      selectedVRFiles: selectedFiles.map((file) => file.path),
                      selectedVRFolders: selectedFolders.map(
                        (folder) => folder.path,
                      ),
                    },
                  });
                }
              }}
              size={isVRSelectionActive ? 'lg' : 'icon'}
              transition={{ duration: 0.2 }}
              variant="outline"
            >
              {!isVRSelectionActive && <CreateAIIcon />}
              <motion.div
                className={cn(
                  'sr-only flex flex-col',
                  isVRSelectionActive && 'not-sr-only',
                )}
                layout
              >
                <span>{t('chat.create')}</span>
                {isVRSelectionActive && (
                  <span className="text-text-secondary text-sm">
                    {t('vectorFs.filesSelected', {
                      count: selectedFiles.length + selectedFolders.length,
                    })}
                  </span>
                )}
              </motion.div>
            </MotionButton>
          </TooltipTrigger>
          <TooltipPortal>
            <TooltipContent side="left">
              <p>{t('chat.create')}</p>
            </TooltipContent>
          </TooltipPortal>
        </Tooltip>
      </TooltipProvider>
      {isVRSelectionActive && (
        <MotionButton
          animate={{ opacity: 1 }}
          className="border-border bg-bg-quaternary text-text-secondary fixed right-[230px] bottom-12 h-[24px] w-[24px] border p-1 hover:text-white"
          initial={{ opacity: 0 }}
          onClick={() => setVRSelectionActive(false)}
          size="icon"
          transition={{
            duration: 0.4,
          }}
          variant="outline"
        >
          <span className="sr-only">{t('common.unselectAll')}</span>
          <XIcon />
        </MotionButton>
      )}
    </div>
  );
};
export default AllFiles;
