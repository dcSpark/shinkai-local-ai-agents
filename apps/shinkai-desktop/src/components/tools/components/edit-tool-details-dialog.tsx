import { zodResolver } from '@hookform/resolvers/zod';
import { PopoverClose } from '@radix-ui/react-popover';
import {
  type ShinkaiTool,
  type ShinkaiToolType,
} from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { useUpdateTool } from '@shinkai_network/shinkai-node-state/v2/mutations/updateTool/useUpdateTool';
import {
  Button,
  Form,
  FormField,
  FormItem,
  FormMessage,
  Input,
  Label,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Textarea,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { z } from 'zod';

import { useAuth } from '../../../store/auth';

const toolDetailsSchema = z.object({
  description: z.string().min(1, 'Description is required'),
  keywords: z.string().optional(),
  previewUrl: z
    .string()
    .url('Must be a valid URL')
    .optional()
    .or(z.literal('')),
  iconUrl: z.string().url('Must be a valid URL').optional().or(z.literal('')),
  version: z
    .string()
    .regex(/^\d+\.\d+\.\d+$/, 'Version must be in x.x.x format'),
});

type ToolDetailsFormSchema = z.infer<typeof toolDetailsSchema>;

interface EditToolDetailsDialogProps {
  tool: ShinkaiTool;
  toolType: ShinkaiToolType;
  toolKey: string;
  fieldName: 'description' | 'keywords' | 'previewUrl' | 'iconUrl' | 'version';
  currentValue: string;
  onUpdate?: () => void;
  className?: string;
}

export default function EditToolDetailsDialog({
  tool,
  toolType,
  toolKey,
  fieldName,
  currentValue,
  onUpdate,
  className,
}: EditToolDetailsDialogProps) {
  const [isOpen, setIsOpen] = useState(false);
  const auth = useAuth((state) => state.auth);

  const { mutateAsync: updateTool, isPending } = useUpdateTool({
    onSuccess: () => {
      toast.success(`Tool ${fieldName} updated successfully`);
      setIsOpen(false);
      if (onUpdate) {
        onUpdate();
      }
    },
    onError: (error) => {
      toast.error(`Failed to update tool ${fieldName}`, {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const form = useForm<ToolDetailsFormSchema>({
    resolver: zodResolver(toolDetailsSchema),
    defaultValues: {
      description:
        fieldName === 'description' ? currentValue : tool.description,
      keywords:
        fieldName === 'keywords'
          ? currentValue
          : 'keywords' in tool
            ? (tool.keywords?.join(', ') ?? '')
            : '',
      previewUrl: fieldName === 'previewUrl' ? currentValue : '',
      iconUrl: fieldName === 'iconUrl' ? currentValue : '',
      version:
        fieldName === 'version'
          ? currentValue
          : 'version' in tool
            ? tool.version
            : '1.0.0',
    },
  });

  const onSubmit = async (data: ToolDetailsFormSchema) => {
    if (fieldName === 'previewUrl' || fieldName === 'iconUrl') {
      toast.info(
        `${fieldName} update functionality requires backend implementation`,
      );
      setIsOpen(false);
      return;
    }

    const toolPayload: Partial<ShinkaiTool> = {};

    if (fieldName === 'description') {
      toolPayload.description = data.description;
    } else if (fieldName === 'keywords') {
      if ('keywords' in tool) {
        (toolPayload as any).keywords = data.keywords
          ? data.keywords.split(',').map((k) => k.trim())
          : [];
      }
    } else if (fieldName === 'version') {
      if ('version' in tool) {
        (toolPayload as any).version = data.version;
      }
    }

    await updateTool({
      toolKey: toolKey,
      toolType: toolType,
      toolPayload: toolPayload as ShinkaiTool,
      isToolEnabled: true,
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    });
  };

  const renderFormField = () => {
    switch (fieldName) {
      case 'description':
        return (
          <FormField
            control={form.control}
            name="description"
            render={({ field }) => (
              <FormItem className="space-y-1">
                <Label className="text-xs font-medium" htmlFor="description">
                  Description
                </Label>
                <Textarea
                  className="resize-none border-none py-2 pt-2 pl-2 text-xs caret-white focus-visible:ring-0 focus-visible:ring-white"
                  id="description"
                  onChange={field.onChange}
                  placeholder="Tool Description"
                  value={field.value}
                />
                <FormMessage />
              </FormItem>
            )}
          />
        );
      case 'keywords':
        return (
          <FormField
            control={form.control}
            name="keywords"
            render={({ field }) => (
              <FormItem className="space-y-1">
                <Label className="text-xs font-medium" htmlFor="keywords">
                  Keywords
                </Label>
                <Input
                  className="!h-[40px] resize-none border-none py-0 pt-0 pl-2 text-xs caret-white focus-visible:ring-0 focus-visible:ring-white"
                  id="keywords"
                  onChange={field.onChange}
                  placeholder="Comma separated keywords"
                  value={field.value}
                />
                <FormMessage />
              </FormItem>
            )}
          />
        );
      case 'previewUrl':
        return (
          <FormField
            control={form.control}
            name="previewUrl"
            render={({ field }) => (
              <FormItem className="space-y-1">
                <Label className="text-xs font-medium" htmlFor="previewUrl">
                  Preview URL
                </Label>
                <Input
                  className="!h-[40px] resize-none border-none py-0 pt-0 pl-2 text-xs caret-white focus-visible:ring-0 focus-visible:ring-white"
                  id="previewUrl"
                  onChange={field.onChange}
                  placeholder="https://example.com/preview.png"
                  value={field.value}
                />
                <FormMessage />
              </FormItem>
            )}
          />
        );
      case 'iconUrl':
        return (
          <FormField
            control={form.control}
            name="iconUrl"
            render={({ field }) => (
              <FormItem className="space-y-1">
                <Label className="text-xs font-medium" htmlFor="iconUrl">
                  Icon URL
                </Label>
                <Input
                  className="!h-[40px] resize-none border-none py-0 pt-0 pl-2 text-xs caret-white focus-visible:ring-0 focus-visible:ring-white"
                  id="iconUrl"
                  onChange={field.onChange}
                  placeholder="https://example.com/icon.png"
                  value={field.value}
                />
                <FormMessage />
              </FormItem>
            )}
          />
        );
      case 'version':
        return (
          <FormField
            control={form.control}
            name="version"
            render={({ field }) => (
              <FormItem className="space-y-1">
                <Label className="text-xs font-medium" htmlFor="version">
                  Version
                </Label>
                <Input
                  className="!h-[40px] resize-none border-none py-0 pt-0 pl-2 text-xs caret-white focus-visible:ring-0 focus-visible:ring-white"
                  id="version"
                  onChange={field.onChange}
                  placeholder="1.0.0"
                  value={field.value}
                />
                <FormMessage />
              </FormItem>
            )}
          />
        );
      default:
        return null;
    }
  };

  return (
    <Popover
      onOpenChange={(open) => {
        if (open) {
          form.reset({
            description:
              fieldName === 'description' ? currentValue : tool.description,
            keywords:
              fieldName === 'keywords'
                ? currentValue
                : 'keywords' in tool
                  ? (tool.keywords?.join(', ') ?? '')
                  : '',
            previewUrl: fieldName === 'previewUrl' ? currentValue : '',
            iconUrl: fieldName === 'iconUrl' ? currentValue : '',
            version:
              fieldName === 'version'
                ? currentValue
                : 'version' in tool
                  ? tool.version
                  : '1.0.0',
          });
        }
        setIsOpen(open);
      }}
      open={isOpen}
    >
      <PopoverTrigger asChild className={cn(className, 'bg-bg-quaternary')}>
        <Button className="h-6 px-2" size="xs" variant="tertiary">
          Edit
          <span className="sr-only">{fieldName}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="flex w-[450px] flex-col gap-4 p-4"
        side="bottom"
        sideOffset={10}
      >
        <h1 className="text-sm leading-none font-semibold tracking-tight">
          Edit {fieldName.charAt(0).toUpperCase() + fieldName.slice(1)}
        </h1>
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(onSubmit)}>
            {renderFormField()}

            <div className="ml-auto flex max-w-[200px] items-center justify-end gap-2">
              <PopoverClose asChild>
                <Button
                  className="flex-1"
                  disabled={isPending}
                  size="xs"
                  variant="outline"
                >
                  Cancel
                </Button>
              </PopoverClose>
              <Button
                className="flex-1"
                disabled={isPending}
                isLoading={isPending}
                size="xs"
                type="submit"
              >
                Update
              </Button>
            </div>
          </form>
        </Form>
      </PopoverContent>
    </Popover>
  );
}
