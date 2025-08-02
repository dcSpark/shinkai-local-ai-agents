import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useImportAgent } from '@shinkai_network/shinkai-node-state/v2/mutations/importAgent/useImportAgent';
import {
  Button,
  buttonVariants,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  FileUploader,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@shinkai_network/shinkai-ui';
import { ImportIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { XIcon } from 'lucide-react';
import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';
import { z } from 'zod';

import { useAnalytics } from '../../lib/posthog-provider';
import { useAuth } from '../../store/auth';

const importAgentFormSchema = z.object({
  file: z.any(),
});
type ImportAgentFormSchema = z.infer<typeof importAgentFormSchema>;

export default function ImportAgentModal() {
  const auth = useAuth((state) => state.auth);
  const { t } = useTranslation();
  const navigate = useNavigate();

  const [isImportModalOpen, setImportModalOpen] = useState(false);
  const { captureAnalyticEvent } = useAnalytics();

  const importAgentForm = useForm<ImportAgentFormSchema>({
    resolver: zodResolver(importAgentFormSchema),
  });

  const { mutateAsync: importAgent, isPending } = useImportAgent({
    onSuccess: (data) => {
      setImportModalOpen(false);
      toast.success('Agent imported successfully', {
        action: {
          label: 'View',
          onClick: () => {
            void navigate(`/agents/edit/${data.agent_id}`);
          },
        },
      });
      captureAnalyticEvent('Agent Imported', undefined);
    },
    onError: (error) => {
      toast.error('Failed to import agent', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const onSubmit = async (data: ImportAgentFormSchema) => {
    await importAgent({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      file: data.file?.[0],
    });
  };

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open) {
          importAgentForm.reset();
          return;
        }
        setImportModalOpen(open);
      }}
      open={isImportModalOpen}
    >
      <DialogTrigger
        className={cn(
          buttonVariants({
            variant: 'outline',
            size: 'sm',
          }),
          'min-w-[100px] gap-1',
        )}
      >
        <ImportIcon className="size-4" />
        <span>{t('agents.importModal.action')} </span>
      </DialogTrigger>
      <DialogContent
        className="max-w-[500px]"
        onInteractOutside={(e) => {
          e.preventDefault();
        }}
      >
        <Button
          className="absolute top-6 right-4"
          onClick={() => {
            setImportModalOpen(false);
          }}
          size="icon"
          variant="tertiary"
        >
          <XIcon className="text-text-secondary h-5 w-5" />
        </Button>
        <div className="px-2 pt-2.5 antialiased">
          <div>
            <DialogHeader>
              <DialogTitle className="text-center">
                {t('agents.importModal.title')}
              </DialogTitle>
            </DialogHeader>
            <Form {...importAgentForm}>
              <form
                className="mt-8 flex flex-col gap-6"
                onSubmit={importAgentForm.handleSubmit(onSubmit)}
              >
                <FormField
                  control={importAgentForm.control}
                  name="file"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel className="sr-only">
                        {t('common.file')}
                      </FormLabel>
                      <FormControl>
                        <FileUploader
                          accept={['zip'].join(',')}
                          descriptionText={t('agents.importModal.chooseFile')}
                          maxFiles={1}
                          onChange={(acceptedFiles) => {
                            field.onChange(acceptedFiles);
                          }}
                          shouldDisableScrolling
                          value={field.value}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <DialogFooter>
                  <Button
                    className="w-full"
                    disabled={isPending}
                    isLoading={isPending}
                    size="auto"
                    type="submit"
                  >
                    {t('agents.importModal.action')}
                  </Button>
                </DialogFooter>
              </form>
            </Form>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
