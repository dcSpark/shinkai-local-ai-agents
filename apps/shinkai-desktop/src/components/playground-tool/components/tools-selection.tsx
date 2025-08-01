import { useGetTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsList/useGetToolsList';
import {
  Badge,
  FormControl,
  FormField,
  FormItem,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Switch,
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
import { InfoCircleIcon } from 'primereact/icons/infocircle';
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

  const { data: toolsList } = useGetTools({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const form = useFormContext<CreateToolCodeFormSchema>();

  return (
    <TooltipProvider delayDuration={0}>
      <Popover>
        <PopoverTrigger asChild>
          <div
            className={cn(
              actionButtonClassnames,
              'w-auto gap-2',
              value.length > 0 && 'bg-cyan-950 hover:bg-cyan-950',
            )}
            role="button"
            tabIndex={0}
          >
            {value.length > 0 ? (
              <Badge className="bg-official-gray-1000 inline-flex size-4 items-center justify-center rounded-full border-gray-200 p-0 text-center text-[10px] text-gray-50">
                {value.length}
              </Badge>
            ) : (
              <ToolsIcon className="size-4" />
            )}
            Tools
          </div>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          className="flex w-full max-w-xl flex-col gap-3 p-4 pr-1 text-xs"
        >
          <div className="flex items-center justify-between pr-3">
            <p className="font-semibold text-white">Available tools</p>
            <div className="flex items-center gap-3">
              <Switch
                checked={value.length === toolsList?.length}
                id="all"
                onCheckedChange={(checked) => {
                  const isAllConfigFilled = toolsList
                    ?.map((tool) => tool.config)
                    .filter((item) => !!item)
                    .flat()
                    ?.map((conf) => ({
                      key_name: conf.BasicConfig.key_name,
                      key_value: conf.BasicConfig.key_value ?? '',
                      required: conf.BasicConfig.required,
                    }))
                    .every(
                      (conf) =>
                        !conf.required ||
                        (conf.required && conf.key_value !== ''),
                    );
                  if (!isAllConfigFilled) {
                    toast.error('Tool configuration', {
                      description:
                        'Please fill in the config required in tool details',
                    });
                    return;
                  }

                  if (checked && toolsList) {
                    onChange(toolsList.map((tool) => tool.tool_router_key));
                  } else {
                    onChange([]);
                  }
                }}
              />
              <label className="text-text-secondary text-xs" htmlFor="all">
                Enabled All
              </label>
            </div>
          </div>

          <div className="flex max-h-[28vh] flex-col gap-2.5 overflow-auto pr-2">
            {toolsList?.map((tool) => (
              <FormField
                control={form.control}
                key={tool.tool_router_key}
                name="tools"
                render={({ field }) => (
                  <FormItem className="flex w-full flex-col gap-3">
                    <FormControl>
                      <div className="flex w-full items-center gap-3">
                        <Switch
                          checked={field.value.includes(tool.tool_router_key)}
                          id={tool.tool_router_key}
                          onCheckedChange={() => {
                            const configs = tool?.config ?? [];
                            if (
                              configs
                                .map((conf) => ({
                                  key_name: conf.BasicConfig.key_name,
                                  key_value: conf.BasicConfig.key_value ?? '',
                                  required: conf.BasicConfig.required,
                                }))
                                .every(
                                  (conf) =>
                                    !conf.required ||
                                    (conf.required && conf.key_value !== ''),
                                )
                            ) {
                              field.onChange(
                                field.value.includes(tool.tool_router_key)
                                  ? field.value.filter(
                                      (item) => item !== tool.tool_router_key,
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
                        <div className="inline-flex flex-1 items-center gap-2 leading-none">
                          <label
                            className="text-text-default max-w-[40ch] truncate text-xs"
                            htmlFor={tool.tool_router_key}
                          >
                            {formatText(tool.name)}
                          </label>
                          <Tooltip>
                            <TooltipTrigger className="flex shrink-0 items-center gap-1">
                              <InfoCircleIcon className="text-text-tertiary h-3 w-3" />
                            </TooltipTrigger>
                            <TooltipPortal>
                              <TooltipContent
                                align="center"
                                alignOffset={-10}
                                className="max-w-md"
                                side="top"
                              >
                                {tool.description}
                              </TooltipContent>
                            </TooltipPortal>
                          </Tooltip>
                        </div>
                        {(tool.config ?? []).length > 0 && (
                          <Tooltip>
                            <TooltipTrigger
                              asChild
                              className="flex shrink-0 items-center gap-1"
                            >
                              <Link
                                className="text-text-secondary hover:text-text-default size-3.5 rounded-lg"
                                to={`/tools/${tool.tool_router_key}`}
                              >
                                <BoltIcon className="size-full" />
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
            ))}
          </div>
        </PopoverContent>
      </Popover>
    </TooltipProvider>
  );
}
