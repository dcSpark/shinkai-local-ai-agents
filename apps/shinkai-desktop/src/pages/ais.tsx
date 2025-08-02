import { zodResolver } from '@hookform/resolvers/zod';
import { DialogClose } from '@radix-ui/react-dialog';
import { DotsVerticalIcon } from '@radix-ui/react-icons';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type EditAIModelFormSchema,
  editAIModelSchema,
} from '@shinkai_network/shinkai-node-state/forms/agents/edit-ai';
import { Models } from '@shinkai_network/shinkai-node-state/lib/utils/models';
import { useRemoveLLMProvider } from '@shinkai_network/shinkai-node-state/v2/mutations/removeLLMProvider/useRemoveLLMProvider';
import { useUpdateLLMProvider } from '@shinkai_network/shinkai-node-state/v2/mutations/updateLLMProvider/useUpdateLLMProvider';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import {
  Badge,
  Button,
  buttonVariants,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Form,
  FormField,
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  TextField,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { CreateAIIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { Edit, Plus, TrashIcon } from 'lucide-react';
import React, { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';

import ProviderIcon from '../components/ais/provider-icon';
import { useOllamaRemoveMutation } from '../lib/shinkai-node-manager/ollama-client';
import { useAuth } from '../store/auth';
import { useShinkaiNodeManager } from '../store/shinkai-node-manager';
import { getModelObject } from './add-ai';

const AIsPage = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();
  const { llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const isLocalShinkaiNodeIsUse = useShinkaiNodeManager(
    (state) => state.isInUse,
  );

  const onAddAgentClick = () => {
    if (isLocalShinkaiNodeIsUse) {
      void navigate('/install-ai-models');
      return;
    }
    void navigate('/add-ai');
  };

  return (
    <div className="h-full">
      <div className="container flex h-full flex-col">
        {/* prettier-ignore */}
        <div className="flex flex-col gap-1 pb-6 pt-10">
          <div className="flex justify-between gap-4">
            <h1 className="font-clash text-3xl font-medium">
              {t('layout.menuItems.manageAis')}
            </h1>
            <div className="flex gap-2">
              <Button
                className="min-w-[100px]"
                onClick={onAddAgentClick}
                size="sm"
              >
                <Plus className="h-4 w-4" />
                <span>{t('llmProviders.add')}</span>
              </Button>
            </div>
          </div>
          <p className="text-text-secondary text-sm">
            {t('aisPage.description')}
          </p>
        </div>

        <div className="flex flex-1 flex-col space-y-3 pb-10">
          {!llmProviders?.length ? (
            <div className="flex grow flex-col items-center justify-center">
              <div className="mb-8 space-y-3 text-center">
                <span aria-hidden className="text-5xl">
                  🤖
                </span>
                <p className="text-2xl font-semibold">
                  {t('llmProviders.notFound.title')}
                </p>
                <p className="text-text-secondary text-center text-sm font-medium">
                  {t('llmProviders.notFound.description')}
                </p>
              </div>

              <Button onClick={onAddAgentClick}>{t('llmProviders.add')}</Button>
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              {llmProviders?.map((llmProvider) => (
                <LLMProviderCard
                  agentApiKey={llmProvider.api_key ?? ''}
                  description={llmProvider.description}
                  externalUrl={llmProvider.external_url ?? ''}
                  key={llmProvider.id}
                  llmProviderId={llmProvider.id}
                  model={llmProvider.model}
                  name={llmProvider.name ?? ''}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default AIsPage;

function LLMProviderCard({
  llmProviderId,
  model,
  externalUrl,
  agentApiKey,
  name,
  description,
}: {
  llmProviderId: string;
  model: string;
  externalUrl: string;
  agentApiKey: string;
  name: string;
  description?: string;
}) {
  const { t } = useTranslation();
  const [isDeleteAgentDrawerOpen, setIsDeleteAgentDrawerOpen] =
    React.useState(false);
  const [isEditAgentDrawerOpen, setIsEditAgentDrawerOpen] =
    React.useState(false);

  const navigate = useNavigate();

  return (
    <React.Fragment>
      <div className="border-divider bg-bg-secondary flex items-center justify-between gap-1 rounded-lg border p-3.5">
        <div className="flex items-center gap-3">
          <div className="flex size-6 items-center justify-center rounded-lg">
            <ProviderIcon
              className="size-full"
              provider={model.split(':')[0]}
            />
          </div>
          <div className="flex flex-col items-baseline gap-2">
            <span className="w-full truncate text-start text-sm">
              {name || llmProviderId}
            </span>
            <Badge className="bg-bg-quaternary truncate text-start text-xs font-normal shadow-none">
              {model}
            </Badge>
            {description && (
              <span className="text-text-secondary truncate text-start text-xs">
                {description}
              </span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-4">
          <TooltipProvider delayDuration={0}>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  onClick={() => {
                    void navigate(`/home`, { state: { llmProviderId } });
                  }}
                  size="xs"
                  variant="outline"
                >
                  <CreateAIIcon className="size-4" />
                  <span className="">Chat</span>
                </Button>
              </TooltipTrigger>

              <TooltipPortal>
                <TooltipContent
                  align="center"
                  className="flex flex-col items-center gap-1"
                  side="right"
                >
                  Create New Chat
                </TooltipContent>
              </TooltipPortal>
            </Tooltip>
          </TooltipProvider>
          <DropdownMenu modal={false}>
            <DropdownMenuTrigger asChild>
              <div
                className={cn(
                  buttonVariants({
                    variant: 'outline',
                    size: 'icon',
                  }),
                  'hover:bg-bg-quaternary border-0',
                )}
                onClick={(event) => {
                  event.stopPropagation();
                }}
                role="button"
                tabIndex={0}
              >
                <span className="sr-only">{t('common.moreOptions')}</span>
                <DotsVerticalIcon className="text-text-secondary" />
              </div>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-[160px] px-2.5 py-2">
              {[
                {
                  name: t('common.edit'),
                  icon: <Edit className="mr-3 h-4 w-4" />,
                  onClick: () => {
                    setIsEditAgentDrawerOpen(true);
                  },
                },
                {
                  name: t('common.delete'),
                  icon: <TrashIcon className="mr-3 h-4 w-4" />,
                  onClick: () => {
                    setIsDeleteAgentDrawerOpen(true);
                  },
                },
              ].map((option) => (
                <React.Fragment key={option.name}>
                  {option.name === 'Delete' && <DropdownMenuSeparator />}
                  <DropdownMenuItem
                    key={option.name}
                    onClick={(event) => {
                      event.stopPropagation();
                      option.onClick();
                    }}
                  >
                    {option.icon}
                    {option.name}
                  </DropdownMenuItem>
                </React.Fragment>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
      <EditLLMProviderDrawer
        agentApiKey={agentApiKey}
        agentExternalUrl={externalUrl}
        agentModelProvider={model.split(':')[0]}
        agentModelType={model.split(':')[1]}
        description={description}
        llmProviderId={llmProviderId}
        name={name}
        onOpenChange={setIsEditAgentDrawerOpen}
        open={isEditAgentDrawerOpen}
      />
      <RemoveLLMProviderModal
        llmProviderId={llmProviderId}
        onOpenChange={setIsDeleteAgentDrawerOpen}
        open={isDeleteAgentDrawerOpen}
      />
    </React.Fragment>
  );
}

const EditLLMProviderDrawer = ({
  open,
  onOpenChange,
  llmProviderId,
  agentModelProvider,
  agentModelType,
  agentExternalUrl,
  agentApiKey,
  description,
  name,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  llmProviderId: string;
  agentModelProvider: string;
  agentModelType: string;
  agentExternalUrl: string;
  agentApiKey: string;
  description?: string;
  name: string;
}) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const form = useForm<EditAIModelFormSchema>({
    resolver: zodResolver(editAIModelSchema),
  });

  useEffect(() => {
    form.reset({
      externalUrl: agentExternalUrl,
      apikey: agentApiKey,
      modelCustom: agentModelProvider,
      modelTypeCustom: agentModelType,
      name: name || '',
      description: description || '',
    });
  }, [
    agentModelProvider,
    agentModelType,
    agentExternalUrl,
    agentApiKey,
    description,
    name,
    form,
  ]);

  const { mutateAsync: updateLLMProvider, isPending } = useUpdateLLMProvider({
    onSuccess: () => {
      onOpenChange(false);
      toast.success(t('llmProviders.success.updateAgent'));
    },
    onError: (error) => {
      toast.error(t('llmProviders.errors.updateAgent'), {
        description: typeof error === 'string' ? error : error.message,
      });
    },
  });

  const submit = async (values: EditAIModelFormSchema) => {
    if (!auth) return;
    const model = getModelObject(values.modelCustom, values.modelTypeCustom);

    await updateLLMProvider({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      agent: {
        api_key: values.apikey,
        external_url: values.externalUrl,
        full_identity_name: `${auth.shinkai_identity}/${auth.profile}/agent/${llmProviderId}`,
        id: llmProviderId,
        model,
        name: values.name,
        description: values.description,
      },
    });
  };

  return (
    <Sheet onOpenChange={onOpenChange} open={open}>
      <SheetContent>
        <SheetHeader className="mb-6">
          <SheetTitle className="font-normal">
            {t('common.update')}{' '}
            <span className="font-medium">{llmProviderId}</span>{' '}
          </SheetTitle>
        </SheetHeader>
        <Form {...form}>
          <form
            className="flex flex-col justify-between space-y-6"
            onSubmit={form.handleSubmit(submit)}
          >
            <div className="flex grow flex-col space-y-3">
              <TextField
                field={{
                  value: llmProviderId,
                  disabled: true,
                }}
                label={t('llmProviders.form.generatedId')}
              />

              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('llmProviders.form.name')}
                  />
                )}
              />

              <FormField
                control={form.control}
                name="description"
                render={({ field }) => (
                  <TextField
                    field={field}
                    helperMessage={t('llmProviders.form.descriptionHelper')}
                    label={t('llmProviders.form.description')}
                  />
                )}
              />

              <FormField
                control={form.control}
                name="externalUrl"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('llmProviders.form.externalUrl')}
                  />
                )}
              />

              <FormField
                control={form.control}
                name="apikey"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('llmProviders.form.apiKey')}
                    type="password"
                  />
                )}
              />

              <FormField
                control={form.control}
                name="modelCustom"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('llmProviders.form.modelProvider')}
                  />
                )}
              />
              <FormField
                control={form.control}
                name="modelTypeCustom"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('llmProviders.form.modelId')}
                  />
                )}
              />
            </div>
            <Button
              className="w-full"
              disabled={isPending}
              isLoading={isPending}
              type="submit"
            >
              {t('common.save')}
            </Button>
          </form>
        </Form>
      </SheetContent>
    </Sheet>
  );
};

const RemoveLLMProviderModal = ({
  open,
  onOpenChange,
  llmProviderId,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  llmProviderId: string;
}) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const { mutateAsync: removeLLMProvider, isPending } = useRemoveLLMProvider({
    onSuccess: () => {
      onOpenChange(false);
      toast.success(t('llmProviders.success.deleteAgent'));
    },
    onError: (error) => {
      toast.error(t('llmProviders.errors.deleteAgent'), {
        description: typeof error === 'string' ? error : error.message,
      });
    },
  });
  const ollamaConfig = { host: 'http://127.0.0.1:11435' };
  const { mutateAsync: removeOllamaModel } = useOllamaRemoveMutation(
    ollamaConfig,
    {
      onSuccess: () => {
        console.log('ollama model removed:', llmProviderId);
      },
      onError: (error) => {
        toast.error(t('llmProviders.errors.deleteAgent'), {
          description: typeof error === 'string' ? error : error.message,
        });
      },
    },
  );
  const {
    llmProviders,
    isSuccess: isSuccessLLMProviders,
    isLoading: isLoadingLLMProviders,
  } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogTitle className="pb-0">
          {t('llmProviders.delete.label')}{' '}
          <span className="font-mono font-medium">{llmProviderId}</span>{' '}
        </DialogTitle>
        <DialogDescription>
          {t('llmProviders.delete.description')}
        </DialogDescription>

        <DialogFooter>
          <div className="flex gap-2 pt-4">
            <DialogClose asChild className="flex-1">
              <Button
                className="min-w-[100px] flex-1"
                size="sm"
                type="button"
                variant="outline"
              >
                {t('common.cancel')}
              </Button>
            </DialogClose>
            <Button
              className="min-w-[100px] flex-1"
              isLoading={isPending || isLoadingLLMProviders}
              onClick={async () => {
                if (!auth) return;
                await removeLLMProvider({
                  nodeAddress: auth?.node_address ?? '',
                  llmProviderId: llmProviderId,
                  token: auth?.api_v2_key ?? '',
                });
                let llmProviderModel = llmProviders.find(
                  (provider) => provider.id === llmProviderId,
                )?.model;
                const isOllama =
                  llmProviderModel?.split(':')[0] === Models.Ollama;
                if (isOllama && llmProviderModel) {
                  llmProviderModel = llmProviderModel.slice(
                    llmProviderModel.indexOf(':') + 1,
                  );
                  await removeOllamaModel({ model: llmProviderModel });
                } else
                  console.warn(
                    'llmProviderModel not found for llmProviderId:',
                    llmProviderId,
                  );
              }}
              size="sm"
              variant="destructive"
            >
              {t('common.delete')}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
