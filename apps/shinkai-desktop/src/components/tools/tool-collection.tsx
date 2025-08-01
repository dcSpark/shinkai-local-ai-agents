import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type GetToolsCategory } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { useDisableAllTools } from '@shinkai_network/shinkai-node-state/v2/mutations/disableAllTools/useDisableAllTools';
import { useEnableAllTools } from '@shinkai_network/shinkai-node-state/v2/mutations/enableAllTools/useEnableAllTools';

import { useGetTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsList/useGetToolsList';
import { useGetSearchTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsSearch/useGetToolsSearch';
import {
  Button,
  buttonVariants,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  SearchInput,
  Skeleton,
  ToggleGroup,
  ToggleGroupItem,
} from '@shinkai_network/shinkai-ui';
import { StoreIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { Eye, EyeOff, MoreVerticalIcon, Plus } from 'lucide-react';
import { useState } from 'react';
import { Link, useNavigate } from 'react-router';
import { toast } from 'sonner';

import { useDebounce } from '../../hooks/use-debounce';
import { useAuth } from '../../store/auth';
import { SHINKAI_STORE_URL } from '../../utils/store';

import { DockerStatus } from './components/docker-status';
import ToolCard from './components/tool-card';
import { useToolsStore } from './context/tools-context';
import ImportToolModal from './import-tool';

const toolsGroup: {
  label: string;
  value: GetToolsCategory;
}[] = [
  {
    label: 'Default Tools',
    value: 'default',
  },
  {
    label: 'My Tools',
    value: 'my_tools',
  },
  {
    label: 'Downloaded',
    value: 'downloaded',
  },
];

export const ToolCollection = () => {
  const auth = useAuth((state) => state.auth);
  const { t } = useTranslation();

  const [searchQuery, setSearchQuery] = useState('');
  const debouncedSearchQuery = useDebounce(searchQuery, 600);
  const isSearchQuerySynced = searchQuery === debouncedSearchQuery;

  const selectedToolCategory = useToolsStore(
    (state) => state.selectedToolCategory,
  );

  const setSelectedToolCategory = useToolsStore(
    (state) => state.setSelectedToolCategory,
  );

  const { data: toolsList, isPending } = useGetTools({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    category: selectedToolCategory === 'all' ? undefined : selectedToolCategory,
  });

  const {
    data: searchToolList,
    isLoading: isSearchToolListPending,
    isSuccess: isSearchToolListSuccess,
  } = useGetSearchTools(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      search: debouncedSearchQuery,
    },
    { enabled: isSearchQuerySynced && !!searchQuery },
  );

  const { mutateAsync: enableAllTools } = useEnableAllTools({
    onSuccess: () => {
      toast.success('All tools were enabled successfully');
    },
    onError: (error) => {
      toast.error(error.response?.data?.message ?? error.message);
    },
  });
  const { mutateAsync: disableAllTools } = useDisableAllTools({
    onSuccess: () => {
      toast.success('All tools were disabled successfully');
    },
    onError: (error) => {
      toast.error(error.response?.data?.message ?? error.message);
    },
  });

  const navigate = useNavigate();

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1 pt-10 pb-6">
        <div className="flex justify-between gap-4">
          <h1 className="font-clash text-3xl font-medium">
            {t('tools.label')}
          </h1>
          <div className="flex gap-2">
            <ImportToolModal />
            <Link
              className={cn(
                buttonVariants({
                  size: 'sm',
                  variant: 'outline',
                }),
              )}
              rel="noreferrer"
              target="_blank"
              to={SHINKAI_STORE_URL}
            >
              <StoreIcon className="size-4" />
              {t('tools.store.label')}
            </Link>
            <Button
              className="min-w-[100px]"
              onClick={() => {
                void navigate('/tools/create');
              }}
              size="sm"
            >
              <Plus className="h-4 w-4" />
              <span>{t('tools.newTool')}</span>
            </Button>
          </div>
        </div>
        <p className="text-text-secondary text-sm">{t('tools.description')}</p>
      </div>
      <SearchInput
        classNames={{
          input: 'bg-transparent',
        }}
        onChange={(e) => {
          setSearchQuery(e.target.value);
        }}
        value={searchQuery}
      />

      {searchQuery && isSearchQuerySynced && searchToolList?.length === 0 && (
        <div className="flex h-20 items-center justify-center">
          <p className="text-text-secondary text-sm">
            {t('tools.emptyState.search.text')}
          </p>
        </div>
      )}
      {searchQuery &&
        isSearchQuerySynced &&
        isSearchToolListSuccess &&
        searchToolList?.length > 0 && (
          <div className="divide-official-gray-780 grid grid-cols-1 divide-y py-4">
            {searchToolList?.map((tool) => (
              <ToolCard key={tool.tool_router_key} tool={tool} />
            ))}
          </div>
        )}
      {!searchQuery && isSearchQuerySynced && (
        <div>
          <div className="flex w-full items-center justify-between gap-3">
            <ToggleGroup
              className="border-divider rounded-full border bg-transparent px-0.5 py-1"
              onValueChange={(value) => {
                setSelectedToolCategory(value as GetToolsCategory);
              }}
              type="single"
              value={selectedToolCategory}
            >
              <ToggleGroupItem
                className="data-[state=on]:bg-official-gray-850 text-text-secondary data-[state=on]:text-text-default rounded-full bg-transparent px-3 py-2.5 text-xs font-medium"
                key="all"
                size="sm"
                value="all"
              >
                All
              </ToggleGroupItem>
              {toolsGroup.map((tool) => (
                <ToggleGroupItem
                  className="data-[state=on]:bg-official-gray-850 text-text-secondary data-[state=on]:text-text-default rounded-full bg-transparent px-3 py-2.5 text-xs font-medium"
                  key={tool.value}
                  size="sm"
                  value={tool.value}
                >
                  {tool.label}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>

            <div className="flex items-center gap-3">
              <DockerStatus />
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    className="text-text-secondary"
                    rounded="lg"
                    size="icon"
                    variant="outline"
                  >
                    <MoreVerticalIcon className="size-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="p-2.5">
                  <DropdownMenuItem
                    className="text-xs"
                    onClick={async () => {
                      await enableAllTools({
                        nodeAddress: auth?.node_address ?? '',
                        token: auth?.api_v2_key ?? '',
                      });
                    }}
                  >
                    <Eye className="mr-2 h-4 w-4" />
                    Enable All Tools
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    className="text-xs"
                    onClick={async () => {
                      await disableAllTools({
                        nodeAddress: auth?.node_address ?? '',
                        token: auth?.api_v2_key ?? '',
                      });
                    }}
                  >
                    <EyeOff className="mr-2 h-4 w-4" />
                    Disable All Tools
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
          <div className="divide-divider grid grid-cols-1 divide-y py-4">
            {toolsList?.length === 0 ? (
              <div className="flex flex-col items-center justify-center gap-2 py-8">
                <p className="text-text-secondary text-sm">
                  {t('tools.noToolsInCategory')}
                </p>
              </div>
            ) : (
              toolsList?.map((tool) => (
                <ToolCard key={tool.tool_router_key} tool={tool} />
              ))
            )}
          </div>
        </div>
      )}
      {(isPending || !isSearchQuerySynced || isSearchToolListPending) && (
        <div className="divide-divider grid grid-cols-1 divide-y py-4">
          {Array.from({ length: 8 }).map((_, idx) => (
            <div
              className={cn(
                'grid animate-pulse grid-cols-[1fr_40px_115px_36px] items-center gap-5 rounded-xs px-2 py-3 pr-4 text-left text-sm',
              )}
              key={idx}
            >
              <div className="flex w-full flex-1 flex-col gap-3">
                <Skeleton className="h-4 w-36 rounded-xs" />
                <div className="flex flex-col gap-1">
                  <Skeleton className="h-3 w-full rounded-xs" />
                  <Skeleton className="h-3 w-2/4 rounded-xs" />
                </div>
              </div>
              <Skeleton className="h-7 w-full rounded-md" />
              <Skeleton className="h-7 w-10 rounded-md" />
              <Skeleton className="h-5 w-[36px] rounded-full" />
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
