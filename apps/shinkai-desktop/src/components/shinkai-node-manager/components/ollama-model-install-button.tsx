import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { removeLLMProvider } from '@shinkai_network/shinkai-node-state/v2/mutations/removeLLMProvider/index';
import { useSyncOllamaModels } from '@shinkai_network/shinkai-node-state/v2/mutations/syncOllamaModels/useSyncOllamaModels';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { Button, Progress } from '@shinkai_network/shinkai-ui';
import { useMap } from '@shinkai_network/shinkai-ui/hooks';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { motion } from 'framer-motion';
import { CheckCircle, Download, Loader2, Minus } from 'lucide-react';
import { type ModelResponse, type ProgressResponse } from 'ollama/browser';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';

import {
  useOllamaListQuery,
  useOllamaPullingQuery,
  useOllamaPullMutation,
  useOllamaRemoveMutation,
} from '../../../lib/shinkai-node-manager/ollama-client';
import { ALLOWED_OLLAMA_MODELS } from '../../../lib/shinkai-node-manager/ollama-models';
import { useShinkaiNodeGetOllamaApiUrlQuery } from '../../../lib/shinkai-node-manager/shinkai-node-manager-client';
import { useAuth } from '../../../store/auth';
export const OllamaModelInstallButton = ({ model }: { model: string }) => {
  const { data: ollamaApiUrl } = useShinkaiNodeGetOllamaApiUrlQuery();

  const ollamaConfig = { host: ollamaApiUrl || 'http://127.0.0.1:11435' };
  const auth = useAuth((auth) => auth.auth);
  const { t } = useTranslation();
  const { isLoading: isOllamaListLoading, data: installedOllamaModels } =
    useOllamaListQuery(ollamaConfig);
  const { data: pullingModelsMap } = useOllamaPullingQuery();
  const { mutateAsync: ollamaPull } = useOllamaPullMutation(ollamaConfig, {
    onSuccess: (data, input) => {
      void handlePullProgress(input.model, data);
    },
  });
  const { mutateAsync: syncOllamaModels } = useSyncOllamaModels();
  const { data: llmProviders, isSuccess: isSuccessLLMProviders } =
    useGetLLMProviders({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    });
  const { mutateAsync: ollamaRemove } = useOllamaRemoveMutation(ollamaConfig, {
    onSuccess: (_, input) => {
      toast.success(
        t('shinkaiNode.models.success.modelRemoved', {
          modelName: input.model,
        }),
      );
      installedOllamaModelsMap.delete(input.model);
    },
    onError: (error, input) => {
      toast.error(
        t('shinkaiNode.models.errors.modelRemoved', {
          modelName: input.model,
        }),
        {
          description: error.message,
        },
      );
    },
  });
  const installedOllamaModelsMap = useMap<string, ModelResponse>();
  const handlePullProgress = async (
    model: string,
    progressIterator: AsyncGenerator<ProgressResponse>,
  ): Promise<void> => {
    try {
      for await (const progress of progressIterator) {
        if (!progress) {
          continue;
        }
        if (progress.status === 'success') {
          toast.success(
            t('shinkaiNode.models.success.modelInstalled', {
              modelName: model,
            }),
          );
          if (auth) {
            await syncOllamaModels({
              nodeAddress: auth?.node_address ?? '',
              token: auth?.api_v2_key ?? '',
              allowedModels: ALLOWED_OLLAMA_MODELS,
            });
          }
          break;
        } else if (progress.status === 'error') {
          toast.error(
            t('shinkaiNode.models.errors.modelInstalled', {
              modelName: model,
            }),
          );
        }
      }
    } catch (error) {
      toast.error(
        t('shinkaiNode.models.errors.modelInstalled', {
          modelName: model,
        }),
        {
          description: error?.toString(),
        },
      );
    }
  };
  const getProgress = (progress: ProgressResponse): number => {
    return Math.ceil((100 * (progress.completed ?? 0)) / (progress.total ?? 1));
  };
  useEffect(() => {
    installedOllamaModels?.models &&
      installedOllamaModels.models.forEach((modelResponse) => {
        installedOllamaModelsMap.set(modelResponse.name, modelResponse);
      });
  }, [installedOllamaModels?.models, installedOllamaModelsMap]);
  return (
    <div className="flex w-full items-center justify-center">
      {isOllamaListLoading ? (
        <Loader2 className="animate-spin" />
      ) : installedOllamaModelsMap.has(model) ? (
        <RemoveAIModelButton
          onClick={async () => {
            // Now we depend on the llmProviders query to be successful to remove the model from ollama
            if (!isSuccessLLMProviders) return;
            await ollamaRemove({ model: model });
            const llmProviderId = llmProviders.find((provider) =>
              provider.model.endsWith(model),
            )?.id;
            if (llmProviderId) {
              await removeLLMProvider({
                nodeAddress: auth?.node_address ?? '',
                token: auth?.api_v2_key ?? '',
                llmProviderId: llmProviderId,
              });
              console.log('llmProviderId removed for model:', model);
            } else console.warn('llmProviderId not found for model:', model);
          }}
        />
      ) : pullingModelsMap?.get(model) ? (
        <div className="flex flex-col items-center gap-1">
          <span className="text-text-secondary text-xs">
            {getProgress(pullingModelsMap.get(model) as ProgressResponse) + '%'}
          </span>
          <Progress
            className="h-2 w-full bg-gray-200 [&>*]:bg-gray-100"
            value={getProgress(pullingModelsMap.get(model) as ProgressResponse)}
          />
          <span className="text-text-secondary text-xs">
            {pullingModelsMap.get(model)?.status}
          </span>
        </div>
      ) : (
        <Button
          className="w-full py-1.5 text-sm"
          onClick={() => ollamaPull({ model: model })}
          size="auto"
          variant={'outline'}
        >
          <Download className="mr-2 h-4 w-4" />
          {t('common.install')}
        </Button>
      )}
    </div>
  );
};

const MotionButton = motion(Button);

function RemoveAIModelButton({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation();
  const [isHovered, setIsHovered] = useState(false);
  return (
    <MotionButton
      className={cn(
        'w-full bg-green-900/70 py-1.5 text-sm hover:border-red-800 hover:bg-red-700/50 hover:text-red-50',
      )}
      layout
      onClick={onClick}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      size="auto"
      variant="outline"
    >
      {isHovered ? (
        <Minus className="mr-2 h-3 w-3" />
      ) : (
        <CheckCircle className="mr-2 h-3 w-3 text-white" />
      )}
      {isHovered ? t('common.remove') : t('common.installed')}
    </MotionButton>
  );
}
