import { useGetTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsList/useGetToolsList';
import {
  Badge,
  Button,
  Checkbox,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  FormControl,
  FormField,
  FormItem,
  SearchInput,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { ToolsIcon } from '@shinkai_network/shinkai-ui/assets';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { BoltIcon } from 'lucide-react';

import { useMemo, useState } from 'react';
import { useFormContext } from 'react-hook-form';
import { Link } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';
import { actionButtonClassnames } from '../../chat/conversation-footer';
import { type CreateToolCodeFormSchema } from '../hooks/use-tool-code';

export function ToolsSelection({
  value,
  onChange,
}: {
  value: string[];
  onChange: (value: string[]) => void;
}) {
  const auth = useAuth((state) => state.auth);
  const [isOpen, setIsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');

  const { data: toolsList } = useGetTools({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const form = useFormContext<CreateToolCodeFormSchema>();

  const filteredTools = useMemo(() => {
    if (!toolsList) return [];
    if (!searchQuery.trim()) return toolsList;

    const query = searchQuery.toLowerCase();
    return toolsList.filter(
      (tool) =>
        tool.name.toLowerCase().includes(query) ||
        tool.description?.toLowerCase().includes(query) ||
        tool.tool_router_key.toLowerCase().includes(query),
    );
  }, [toolsList, searchQuery]);

  const handleEnableAll = (checked: boolean) => {
    const targetTools = searchQuery.trim() ? filteredTools : toolsList;
    const isAllConfigFilled = targetTools
      ?.map((tool) => tool.config)
      .filter((item) => !!item)
      .flat()
      ?.map((conf) => ({
        key_name: conf.BasicConfig.key_name,
        key_value: conf.BasicConfig.key_value ?? '',
        required: conf.BasicConfig.required,
      }))
      .every(
        (conf) => !conf.required || (conf.required && conf.key_value !== ''),
      );

    if (!isAllConfigFilled) {
      toast.error('Tool configuration', {
        description: 'Please fill in the config required in tool details',
      });
      return;
    }

    if (checked && targetTools) {
      const toolsToAdd = targetTools.map((tool) => tool.tool_router_key);
      const uniqueTools = Array.from(new Set([...value, ...toolsToAdd]));
      onChange(uniqueTools);
    } else if (targetTools) {
      const toolsToRemove = new Set(
        targetTools.map((tool) => tool.tool_router_key),
      );
      onChange(value.filter((key) => !toolsToRemove.has(key)));
    }
  };

  return (
    <TooltipProvider delayDuration={0}>
      <Dialog open={isOpen} onOpenChange={setIsOpen}>
        <DialogTrigger asChild>
          <div
            className={cn(
              actionButtonClassnames,
              'w-auto gap-2',
              value.length > 0 &&
                'bg-gray-900 text-cyan-400 hover:bg-gray-900 hover:text-cyan-500',
            )}
            role="button"
            tabIndex={0}
          >
            {value.length > 0 ? (
              <Badge className="bg-bg-dark border-divider text-text-default inline-flex size-4 items-center justify-center rounded-full p-0 text-center text-[10px]">
                {value.length}
              </Badge>
            ) : (
              <ToolsIcon className="size-4" />
            )}
            Tools
          </div>
        </DialogTrigger>
        <DialogContent className="flex max-h-[85vh] w-full max-w-2xl flex-col gap-4 p-6">
          <DialogHeader>
            <DialogTitle className="text-lg font-semibold">
              Select Tools
            </DialogTitle>
            <DialogDescription className="text-text-secondary text-sm">
              Choose the tools you want to enable for this agent.{' '}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-2">
            <div className="flex flex-col gap-3">
              <SearchInput
                className="flex-1"
                classNames={{ input: 'bg-transparent' }}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Search tools by name or description..."
                value={searchQuery}
              />
              <div className="flex h-10 items-center justify-between gap-2 px-3 pt-1">
                <span className="text-text-default text-sm">
                  {value.length} tool{value.length > 1 ? 's' : ''} selected
                </span>
                <div>
                  <Button
                    variant="tertiary"
                    size="xs"
                    onClick={() => {
                      onChange([]);
                    }}
                  >
                    Clear
                  </Button>
                  <Button
                    variant="tertiary"
                    size="xs"
                    onClick={() => {
                      handleEnableAll(true);
                    }}
                  >
                    Select All
                  </Button>
                </div>
              </div>
            </div>

            <div className="flex max-h-[calc(85vh-240px)] min-h-[300px] flex-col gap-2.5 overflow-auto pr-2">
              {filteredTools.length === 0 ? (
                <div className="text-text-secondary flex flex-1 items-center justify-center text-sm">
                  {searchQuery.trim()
                    ? 'No tools found matching your search'
                    : 'No tools available'}
                </div>
              ) : (
                filteredTools.map((tool) => (
                  <FormField
                    control={form.control}
                    key={tool.tool_router_key}
                    name="tools"
                    render={({ field }) => (
                      <FormItem className="flex w-full flex-col gap-3">
                        <FormControl>
                          <div className="border-divider hover:bg-bg-secondary flex w-full items-center gap-3 rounded-lg border p-3 transition-colors">
                            <Checkbox
                              checked={field.value.includes(
                                tool.tool_router_key,
                              )}
                              id={tool.tool_router_key}
                              onCheckedChange={() => {
                                const configs = tool?.config ?? [];
                                if (
                                  configs
                                    .map((conf) => ({
                                      key_name: conf.BasicConfig.key_name,
                                      key_value:
                                        conf.BasicConfig.key_value ?? '',
                                      required: conf.BasicConfig.required,
                                    }))
                                    .every(
                                      (conf) =>
                                        !conf.required ||
                                        (conf.required &&
                                          conf.key_value !== ''),
                                    )
                                ) {
                                  field.onChange(
                                    field.value.includes(tool.tool_router_key)
                                      ? field.value.filter(
                                          (item) =>
                                            item !== tool.tool_router_key,
                                        )
                                      : [...field.value, tool.tool_router_key],
                                  );
                                  return;
                                }
                                toast.error('Tool configuration is required', {
                                  description:
                                    'Please fill in the config required in tool details',
                                });
                              }}
                            />
                            <div className="flex flex-1 flex-col gap-1">
                              <label
                                className="text-text-default cursor-pointer text-sm font-medium"
                                htmlFor={tool.tool_router_key}
                              >
                                {formatText(tool.name)}
                              </label>

                              {tool.description && (
                                <p className="text-text-secondary line-clamp-2 text-xs">
                                  {tool.description}
                                </p>
                              )}
                            </div>
                            {(tool.config ?? []).length > 0 && (
                              <Tooltip>
                                <TooltipTrigger
                                  asChild
                                  className="flex shrink-0 items-center gap-1"
                                >
                                  <Link
                                    className="text-text-secondary hover:text-text-default flex size-8 items-center justify-center rounded-lg transition-colors hover:bg-gray-800"
                                    to={`/tools/${tool.tool_router_key}`}
                                  >
                                    <BoltIcon className="size-4" />
                                  </Link>
                                </TooltipTrigger>
                                <TooltipPortal>
                                  <TooltipContent
                                    align="center"
                                    alignOffset={-10}
                                    className="max-w-md"
                                    side="top"
                                  >
                                    <p>Configure tool</p>
                                  </TooltipContent>
                                </TooltipPortal>
                              </Tooltip>
                            )}
                          </div>
                        </FormControl>
                      </FormItem>
                    )}
                  />
                ))
              )}
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </TooltipProvider>
  );
}
