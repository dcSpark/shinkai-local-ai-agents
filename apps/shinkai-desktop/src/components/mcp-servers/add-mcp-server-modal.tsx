import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type McpServer,
  McpServerType,
} from '@shinkai_network/shinkai-message-ts/api/mcp-servers/types';
import { type AddMcpServerInput } from '@shinkai_network/shinkai-node-state/v2/mutations/addMcpServer/types';
import { useAddMcpServer } from '@shinkai_network/shinkai-node-state/v2/mutations/addMcpServer/useAddMcpServer';
import { type ImportMCPServerFromGithubURLOutput } from '@shinkai_network/shinkai-node-state/v2/mutations/importMCPServerFromGithubURL/types';
import { type UpdateMcpServerInput } from '@shinkai_network/shinkai-node-state/v2/mutations/updateMcpServer/types';
import { useUpdateMcpServer } from '@shinkai_network/shinkai-node-state/v2/mutations/updateMcpServer/useUpdateMcpServer';
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  TextField,
} from '@shinkai_network/shinkai-ui';
import { AlertTriangle, PlusCircle, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useFieldArray, useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { z } from 'zod';

import { useAnalytics } from '../../lib/posthog-provider';
import { useAuth } from '../../store/auth';

// Define a schema for environment variable entry
interface EnvVar {
  key: string;
  value: string;
}

const envVarSchema = z.object({
  key: z.string(),
  value: z.string(),
});

// Define the form schema based on the McpServerType
const formSchema = z.discriminatedUnion('type', [
  z.object({
    name: z.string().min(1, { message: 'Name is required' }),
    type: z.literal(McpServerType.Command),
    command: z.string().min(1, { message: 'Command is required' }),
    env: z.array(envVarSchema),
  }),
  z.object({
    name: z.string().min(1, { message: 'Name is required' }),
    type: z.literal(McpServerType.Sse),
    url: z.string().url({ message: 'Invalid URL format' }),
  }),
  z.object({
    name: z.string().min(1, { message: 'Name is required' }),
    type: z.literal(McpServerType.Http),
    url: z.string().url({ message: 'Invalid URL format' }),
  }),
]);

type FormSchemaType = z.infer<typeof formSchema>;

interface AddMcpServerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
  initialData?: ImportMCPServerFromGithubURLOutput | McpServer;
  mode: 'Create' | 'Update';
}

export const AddMcpServerModal = ({
  isOpen,
  onClose,
  onSuccess,
  initialData,
  mode,
}: AddMcpServerModalProps) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const form = useForm<FormSchemaType>({
    resolver: zodResolver(formSchema),
    defaultValues: {
      name: '',
      type: McpServerType.Command,
      command: '',
      env: [],
    },
  });

  useEffect(() => {
    if (isOpen) {
      if (initialData) {
        // Branch for Command types (either Import or McpServer variant)
        if (initialData.type === McpServerType.Command) {
          const name = initialData.name || '';
          const command = (initialData as { command?: string }).command || '';
          const envRecord = (initialData as { env?: Record<string, any> }).env;

          let envArray: EnvVar[] = [];
          if (envRecord && typeof envRecord === 'object') {
            const transformedEnv = Object.entries(envRecord).map(
              ([key, value]) => ({
                key,
                value: String(value),
              }),
            );
            if (transformedEnv.length > 0) {
              envArray = transformedEnv;
            }
          }

          form.reset({
            name,
            type: McpServerType.Command,
            command,
            env: envArray,
          });
        } else if (initialData.type === McpServerType.Sse) {
          // initialData is Extract<McpServer, { type: McpServerType.Sse }>
          const sseData = initialData as Extract<
            McpServer,
            { type: McpServerType.Sse }
          >; // Cast for clarity and safety
          form.reset({
            name: sseData.name || '',
            type: McpServerType.Sse,
            url: sseData.url || '',
          });
        } else {
          // Fallback to a default command form, trying to preserve name if possible.
          const name = (initialData as any).name || '';
          form.reset({
            name: name,
            type: McpServerType.Command,
            command: '',
            env: [],
          });
        }
      } else {
        // No initialData, reset to a clean command form
        form.reset({
          name: '',
          type: McpServerType.Command,
          command: '',
          env: [],
        });
      }
    } else {
      // Modal is closed, reset to a clean default state
      form.reset({
        name: '',
        type: McpServerType.Command,
        command: '',
        env: [],
      });
    }
  }, [initialData, isOpen, form]);

  const { fields, append, remove } = useFieldArray({
    control: form.control,
    name: 'env',
  });

  const serverType = form.watch('type');

  const { captureAnalyticEvent } = useAnalytics();

  const { mutateAsync: addMcpServer } = useAddMcpServer({
    onSuccess: () => {
      toast.success(t('mcpServers.addSuccess'));
      onSuccess();
      setIsSubmitting(false);
      form.reset();
      captureAnalyticEvent('MCP Server Added', undefined);
    },
    onError: (error) => {
      toast.error(t('mcpServers.addFailed'), {
        description: error.response?.data?.message ?? error.message,
      });
      setIsSubmitting(false);
    },
  });

  const { mutateAsync: updateMcpServer } = useUpdateMcpServer({
    onSuccess: () => {
      toast.success(t('mcpServers.updateSuccess'));
      onSuccess();
      setIsSubmitting(false);
      form.reset();
    },
    onError: (error: Error) => {
      toast.error(t('mcpServers.updateFailed'), {
        description: error?.message,
      });
      setIsSubmitting(false);
    },
  });

  const onSubmit = async (values: FormSchemaType) => {
    if (!auth) return;
    setIsSubmitting(true);
    try {
      // Base structure for the payload, common to add and potentially update
      const commonPayload = {
        name: values.name,
        // Retain is_enabled from initialData if available (for updates or GitHub import),
        // otherwise default to true for new creations.
        is_enabled:
          typeof initialData?.is_enabled === 'boolean'
            ? initialData.is_enabled
            : true,
      };

      let specificPayload;
      if (values.type === McpServerType.Command) {
        const envRecord: Record<string, string> = {};
        if (values.env && values.env.length > 0) {
          values.env.forEach(({ key, value }) => {
            if (key.trim()) {
              // Ensure key is not empty or just whitespace
              envRecord[key.trim()] = value;
            }
          });
        }
        specificPayload = {
          ...commonPayload,
          type: McpServerType.Command as const,
          command: values.command,
          env: Object.keys(envRecord).length > 0 ? envRecord : undefined,
        };
      } else if (values.type === McpServerType.Http) {
        specificPayload = {
          ...commonPayload,
          type: McpServerType.Http as const,
          url: values.url,
        };
      } else {
        specificPayload = {
          ...commonPayload,
          type: McpServerType.Sse as const,
          url: values.url,
        };
      }

      // Construct the full payload for the mutation hook
      const mutationInput = {
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        ...specificPayload,
      } as AddMcpServerInput;

      if (mode === 'Create') {
        await addMcpServer(mutationInput);
      } else {
        // Update mode
        const serverId = (initialData as McpServer)?.id;
        if (serverId === undefined) {
          toast.error('Failed to update MCP server: Server ID is missing.');
          setIsSubmitting(false);
          return;
        }
        const updateInput: UpdateMcpServerInput = {
          nodeAddress: auth.node_address,
          token: auth.api_v2_key,
          id: serverId,
          ...specificPayload,
        };
        await updateMcpServer(updateInput);
      }
    } catch (error) {
      console.error(
        mode === 'Create'
          ? 'Failed to add MCP server:'
          : 'Failed to update MCP server:',
        error,
      );
    }
  };

  return (
    <Dialog onOpenChange={(open) => !open && onClose()} open={isOpen}>
      <DialogContent className="max-w-xl" showCloseButton>
        <DialogHeader className="flex flex-col gap-2">
          <DialogTitle>
            {mode === 'Create' ? t('mcpServers.add') : t('mcpServers.update')}
          </DialogTitle>
          <DialogDescription className="sr-only">
            {mode === 'Create'
              ? t('mcpServers.addDescription')
              : t('mcpServers.updateDescription', { name: initialData?.name })}
          </DialogDescription>
        </DialogHeader>
        {mode === 'Update' && (
          <div className="mt-2 flex items-start rounded-md border border-yellow-500/50 bg-yellow-500/10 p-3 text-sm text-yellow-200">
            <AlertTriangle className="mr-2 h-5 w-5 flex-shrink-0 text-yellow-400" />
            <div>{t('mcpServers.updateWarningDescription')}</div>
          </div>
        )}
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(onSubmit)}>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <TextField field={field} label="Server Name" type="text" />
              )}
            />

            <FormField
              control={form.control}
              name="type"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Connection Type</FormLabel>
                  <Select
                    defaultValue={field.value}
                    onValueChange={(value) => {
                      const newType = value as McpServerType;
                      field.onChange(newType);
                      form.setValue('type', newType, { shouldValidate: true });

                      if (newType === McpServerType.Command) {
                        form.setValue(
                          'command',
                          form.getValues('command') || '',
                        );
                        form.setValue(
                          'env',
                          form.getValues('env') || [{ key: '', value: '' }],
                        );
                        form.clearErrors('url');
                        form.unregister('url');
                      } else if (newType === McpServerType.Sse) {
                        form.setValue('url', form.getValues('url') || '');
                        form.clearErrors('command');
                        form.clearErrors('env');
                        form.unregister('command');
                        form.unregister('env');
                      } else if (newType === McpServerType.Http) {
                        form.setValue('url', form.getValues('url') || '');
                        form.clearErrors('command');
                        form.clearErrors('env');
                        form.unregister('command');
                        form.unregister('env');
                      }
                    }}
                    value={field.value}
                  >
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue
                          placeholder={t('mcpServers.selectServerType')}
                        />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      <SelectItem value={McpServerType.Command}>
                        Standard IO
                      </SelectItem>
                      <SelectItem value={McpServerType.Sse}>
                        Server-Sent Events (SSE)
                      </SelectItem>
                      <SelectItem value={McpServerType.Http}>HTTP</SelectItem>
                    </SelectContent>
                  </Select>
                  <FormMessage />
                </FormItem>
              )}
            />

            {serverType === McpServerType.Command && (
              <>
                <FormField
                  control={form.control}
                  name="command"
                  render={({ field }) => (
                    <TextField field={field} label="Command" type="text" />
                  )}
                />

                <div className="max-h-[300px] space-y-3 overflow-y-auto pt-2">
                  <h2 className="text-sm font-medium">
                    {t('mcpServers.environmentVariables')}
                  </h2>
                  {fields.map((item, index) => {
                    const currentEnvKey = form.watch(`env.${index}.key`) || '';
                    const sensitiveKeywords = ['secret', 'key', 'password'];
                    const isSensitive = sensitiveKeywords.some((keyword) =>
                      currentEnvKey.toLowerCase().includes(keyword),
                    );

                    return (
                      <div
                        className="flex items-center gap-2 [&>div]:flex-1"
                        key={item.id}
                      >
                        <FormField
                          control={form.control}
                          name={`env.${index}.key`}
                          render={({ field }) => (
                            <TextField field={field} label="Key" type="text" />
                          )}
                        />
                        <FormField
                          control={form.control}
                          name={`env.${index}.value`}
                          render={({ field }) => (
                            <TextField
                              field={field}
                              label="Value"
                              type={isSensitive ? 'password' : 'text'}
                            />
                          )}
                        />
                        <Button
                          className="mb-1.5 h-9 w-9 shrink-0"
                          onClick={() => remove(index)}
                          size="icon"
                          type="button"
                          variant="tertiary"
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    );
                  })}
                  <div className="flex flex-col items-center justify-center gap-2">
                    {fields.length === 0 && (
                      <p className="text-text-secondary text-sm">
                        {t('mcpServers.noEnvironmentVariablesAdded')}
                      </p>
                    )}
                    <Button
                      className="min-w-20"
                      onClick={(e) => {
                        e.stopPropagation();
                        append({ key: '', value: '' });
                      }}
                      size="sm"
                      type="button"
                      variant="outline"
                    >
                      <PlusCircle className="h-3.5 w-3.5" />
                      <span>{t('mcpServers.addVariable')}</span>
                    </Button>
                  </div>
                </div>
              </>
            )}

            {serverType === McpServerType.Sse ||
              (serverType === McpServerType.Http && (
                <FormField
                  control={form.control}
                  name="url"
                  render={({ field }) => (
                    <TextField field={field} label="Server URL" type="text" />
                  )}
                />
              ))}

            <DialogFooter className="mt-6">
              <Button
                className="min-w-[120px]"
                disabled={isSubmitting}
                onClick={onClose}
                size="md"
                type="button"
                variant="outline"
              >
                {t('common.cancel')}
              </Button>
              <Button
                className="min-w-[140px]"
                disabled={
                  isSubmitting ||
                  (form.formState.isSubmitted && !form.formState.isValid)
                }
                isLoading={isSubmitting}
                size="md"
                type="submit"
              >
                {isSubmitting
                  ? mode === 'Create'
                    ? t('common.adding')
                    : t('common.updating')
                  : mode === 'Create'
                    ? t('mcpServers.add')
                    : t('mcpServers.update')}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
};
