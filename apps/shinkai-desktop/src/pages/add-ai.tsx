import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type LLMProviderInterface } from '@shinkai_network/shinkai-message-ts/api/jobs/types';
import {
  addAiModelFormDefault,
  type AddAiModelFormSchema,
  addAiModelSchema,
} from '@shinkai_network/shinkai-node-state/forms/agents/add-ai';
import {
  Models,
  modelsConfig,
} from '@shinkai_network/shinkai-node-state/lib/utils/models';
import { useAddLLMProvider } from '@shinkai_network/shinkai-node-state/v2/mutations/addLLMProvider/useAddLLMProvider';
import { useScanOllamaModels } from '@shinkai_network/shinkai-node-state/v2/queries/scanOllamaModels/useScanOllamaModels';
import {
  Button,
  buttonVariants,
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
  Switch,
  TextField,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { HelpCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useForm, useWatch } from 'react-hook-form';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';

import { useURLQueryParams } from '../hooks/use-url-query-params';
import { useAuth } from '../store/auth';
import { SubpageLayout } from './layout/simple-layout';

const modelOptions: { value: Models; label: string }[] = [
  {
    value: Models.OpenAI,
    label: 'OpenAI',
  },
  {
    value: Models.TogetherComputer,
    label: 'Together AI',
  },
  {
    value: Models.Ollama,
    label: 'Ollama',
  },
  {
    value: Models.Gemini,
    label: 'Gemini',
  },
  {
    value: Models.Groq,
    label: 'Groq',
  },
  {
    value: Models.OpenRouter,
    label: 'OpenRouter',
  },
  {
    value: Models.Exo,
    label: 'Exo',
  },
  {
    value: Models.Claude,
    label: 'Claude',
  },
  {
    value: Models.DeepSeek,
    label: 'DeepSeek',
  },
  {
    value: Models.Grok,
    label: 'Grok',
  },
];

const getGuideUrl = (model: Models) => {
  if (!model) {
    return 'https://docs.shinkai.com/advanced/models';
  }

  const urlMap = {
    [Models.OpenAI]: 'https://docs.shinkai.com/advanced/models/gpt',
    [Models.TogetherComputer]:
      'https://docs.shinkai.com/advanced/models/together-ai',
    [Models.Ollama]: 'https://docs.shinkai.com/advanced/models/ollama',
    [Models.Gemini]: 'https://docs.shinkai.com/advanced/models/gemini',
    [Models.Groq]: 'https://docs.shinkai.com/advanced/models/groq',
    [Models.OpenRouter]: 'https://docs.shinkai.com/advanced/models/openrouter',
    [Models.Exo]: 'https://docs.shinkai.com/advanced/models/exo',
    [Models.Claude]: 'https://docs.shinkai.com/advanced/models/claude',
    [Models.DeepSeek]: 'https://docs.shinkai.com/advanced/models/deepseek',
    [Models.Grok]: 'https://docs.shinkai.com/advanced/models/grok',
  };

  return urlMap[model] || 'https://docs.shinkai.com/advanced/models';
};

export const getModelObject = (
  model: Models | string,
  modelType: string,
): LLMProviderInterface => {
  switch (model) {
    case Models.OpenAI:
      return { OpenAI: { model_type: modelType } };
    case Models.TogetherComputer:
      return { TogetherAI: { model_type: modelType } };
    case Models.Ollama:
      return { Ollama: { model_type: modelType } };
    case Models.Gemini:
      return { Gemini: { model_type: modelType } };
    case Models.Exo:
      return { Exo: { model_type: modelType } };
    case Models.Claude:
      return { Claude: { model_type: modelType } };
    case Models.DeepSeek:
      return { DeepSeek: { model_type: modelType } };
    case Models.Grok:
      return { Grok: { model_type: modelType } };
    default:
      return { [model]: { model_type: modelType } };
  }
};

const AddAIPage = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();
  const query = useURLQueryParams();

  const preSelectedAiProvider = query.get('aiProvider') as Models;
  const addAgentForm = useForm<AddAiModelFormSchema>({
    resolver: zodResolver(addAiModelSchema),
    defaultValues: {
      ...addAiModelFormDefault,
      model: preSelectedAiProvider || undefined,
      modelType:
        (preSelectedAiProvider &&
          modelsConfig[preSelectedAiProvider]?.modelTypes?.[0]?.value) ||
        '',
    },
  });

  const { mutateAsync: addLLMProvider, isPending } = useAddLLMProvider({
    onSuccess: (_, variables) => {
      void navigate('/ais', {
        state: {
          agentName: variables.agent.id,
        },
      });
    },
    onError: (error) => {
      toast.error(t('llmProviders.errors.createAgent'), {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const currentModel = useWatch({
    control: addAgentForm.control,
    name: 'model',
  });
  const isCustomModelMode = useWatch({
    control: addAgentForm.control,
    name: 'isCustomModel',
  });
  const currentModelType = useWatch({
    control: addAgentForm.control,
    name: 'modelType',
  });

  const {
    data: ollamaModels,
    isError: isOllamaModelsError,
    error: ollamaModelsError,
  } = useScanOllamaModels(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    {
      enabled: !isCustomModelMode && currentModel === Models.Ollama,
      retry: 1,
      staleTime: 0,
    },
  );

  useEffect(() => {
    if (isOllamaModelsError) {
      toast.error(t('ollama.errors.failedToFetch'), {
        description: ollamaModelsError?.message,
      });
    }
  }, [isOllamaModelsError, ollamaModelsError?.message]);

  const [modelTypeOptions, setModelTypeOptions] = useState<
    { label: string; value: string }[]
  >([]);
  const [isCustomModelType, setIsCustomModelType] = useState(false);

  useEffect(() => {
    if (isCustomModelMode) {
      addAgentForm.setValue('externalUrl', '');
      return;
    }
    if (currentModel === Models.Ollama) {
      addAgentForm.setValue('externalUrl', modelsConfig[Models.Ollama].apiUrl);
      setModelTypeOptions(
        (ollamaModels ?? [])
          .map((model) => ({
            label: model.model,
            value: model.model,
          }))
          .concat({ label: 'Custom Model', value: 'custom' }),
      );
      return;
    }

    if (currentModel) {
      const modelConfig = modelsConfig[currentModel as Models];
      addAgentForm.setValue('externalUrl', modelConfig.apiUrl);
      setModelTypeOptions(
        modelsConfig[currentModel as Models].modelTypes
          .map((modelType) => ({
            label: modelType.name,
            value: modelType.value,
          }))
          .concat({ label: 'Custom Model', value: 'custom' }),
      );
    }
  }, [currentModel, addAgentForm, isCustomModelMode, ollamaModels]);

  useEffect(() => {
    if (!modelTypeOptions?.length) {
      return;
    }
    addAgentForm.setValue('modelType', modelTypeOptions[0].value);
  }, [modelTypeOptions, addAgentForm]);

  useEffect(() => {
    if (!modelTypeOptions?.length) {
      return;
    }
    addAgentForm.setValue(
      'name',
      currentModelType.replace(/[^a-zA-Z0-9_]/g, ' '),
    );
  }, [addAgentForm, currentModelType, modelTypeOptions?.length]);

  useEffect(() => {
    setIsCustomModelType(currentModelType === 'custom');
  }, [currentModelType]);

  const onSubmit = async (data: AddAiModelFormSchema) => {
    if (!auth) return;
    let model = getModelObject(data.model, data.modelType);
    if (isCustomModelMode && data.modelCustom && data.modelTypeCustom) {
      model = getModelObject(data.modelCustom, data.modelTypeCustom);
    } else if (isCustomModelType && data.modelTypeCustom) {
      model = getModelObject(data.model, data.modelTypeCustom);
    }
    const generatedId = data.name.replace(/[^a-zA-Z0-9_]/g, '_');

    await addLLMProvider({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      agent: {
        api_key: data.apikey,
        external_url: data.externalUrl,
        full_identity_name: `${auth.shinkai_identity}/${auth.profile}/agent/${generatedId}`,
        id: generatedId,
        model,
        name: data.name,
      },
    });
  };

  return (
    <SubpageLayout
      alignLeft
      className="max-w-lg"
      rightElement={
        <a
          className={cn(buttonVariants({ variant: 'outline', size: 'xs' }))}
          href={getGuideUrl(currentModel)}
          rel="noreferrer"
          target="_blank"
        >
          <HelpCircle className="h-4 w-4" />
          Help
        </a>
      }
      title={t('llmProviders.add')}
    >
      <Form {...addAgentForm}>
        <form
          className="space-y-10"
          onSubmit={addAgentForm.handleSubmit(onSubmit)}
        >
          <div className="space-y-6">
            <FormField
              control={addAgentForm.control}
              name="isCustomModel"
              render={({ field }) => (
                <FormItem className="mt-4 flex flex-row items-center justify-center space-x-3 py-1">
                  <FormControl>
                    <Switch
                      checked={field.value}
                      id={'custom-model'}
                      onCheckedChange={field.onChange}
                    />
                  </FormControl>
                  <div
                    className={cn(
                      'text-text-secondary space-y-1 text-sm leading-none',
                      field.value && 'text-white',
                    )}
                  >
                    <label htmlFor="custom-model">
                      {t('llmProviders.form.toggleCustomModel')}
                    </label>
                  </div>
                </FormItem>
              )}
            />
            {!isCustomModelMode && (
              <FormField
                control={addAgentForm.control}
                name="model"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      {t('llmProviders.form.modelProvider')}
                    </FormLabel>
                    <Select onValueChange={field.onChange} value={field.value}>
                      <FormControl>
                        <SelectTrigger>
                          <SelectValue placeholder={' '} />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent className="max-h-[800px]">
                        {modelOptions.map((model) => (
                          <SelectItem
                            key={model.value}
                            value={model.value.toString()}
                          >
                            {model.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </FormItem>
                )}
              />
            )}

            {!isCustomModelMode && (
              <FormField
                control={addAgentForm.control}
                name="modelType"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t('llmProviders.form.modelType')}</FormLabel>
                    <Select
                      defaultValue={field.value as unknown as string}
                      onValueChange={field.onChange}
                      value={field.value}
                    >
                      <FormControl>
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent className="max-h-[150px] overflow-y-auto text-xs">
                        {modelTypeOptions.map((modelType) => (
                          <SelectItem
                            key={modelType.value}
                            value={modelType.value}
                          >
                            {modelType.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <FormMessage />
                  </FormItem>
                )}
              />
            )}
            {isCustomModelMode && (
              <>
                <FormField
                  control={addAgentForm.control}
                  name="modelCustom"
                  render={({ field }) => (
                    <TextField
                      field={field}
                      label={t('llmProviders.form.modelName')}
                    />
                  )}
                />
                <FormField
                  control={addAgentForm.control}
                  name="modelTypeCustom"
                  render={({ field }) => (
                    <TextField
                      field={field}
                      label={t('llmProviders.form.modelId')}
                    />
                  )}
                />
              </>
            )}

            {isCustomModelType && (
              <FormField
                control={addAgentForm.control}
                name="modelTypeCustom"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('llmProviders.form.customModelType')}
                  />
                )}
              />
            )}

            <FormField
              control={addAgentForm.control}
              name="name"
              render={({ field }) => (
                <TextField
                  autoFocus
                  field={field}
                  helperMessage={
                    <span>
                      {t('llmProviders.form.generatedId')}:{' '}
                      {addAgentForm
                        .watch('name')
                        .replace(/[^a-zA-Z0-9_]/g, '_')}
                    </span>
                  }
                  label={t('llmProviders.form.name')}
                />
              )}
            />
            <FormField
              control={addAgentForm.control}
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
              control={addAgentForm.control}
              name="externalUrl"
              render={({ field }) => (
                <TextField
                  field={field}
                  label={t('llmProviders.form.externalUrl')}
                />
              )}
            />
            <FormField
              control={addAgentForm.control}
              name="apikey"
              render={({ field }) => (
                <TextField
                  classes={{
                    formItem: 'flex-1',
                  }}
                  field={field}
                  label={t('llmProviders.form.apiKey')}
                  type="password"
                />
              )}
            />
          </div>

          <div className="flex flex-col items-center gap-4">
            <Button
              className="w-full"
              disabled={isPending}
              isLoading={isPending}
              size="sm"
              type="submit"
            >
              Test & Add AI
            </Button>
          </div>
        </form>
      </Form>
    </SubpageLayout>
  );
};
export default AddAIPage;
