import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { buildInboxIdFromJobId } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { DEFAULT_CHAT_CONFIG } from '@shinkai_network/shinkai-node-state/v2/constants';
import { useCreateJob } from '@shinkai_network/shinkai-node-state/v2/mutations/createJob/useCreateJob';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { Badge, buttonVariants } from '@shinkai_network/shinkai-ui';
import { invoke } from '@tauri-apps/api/core';
import { motion } from 'framer-motion';
import { ArrowUpRight } from 'lucide-react';
import { useEffect } from 'react';
import { Link, useNavigate } from 'react-router';
import { toast } from 'sonner';

import { useSetJobScope } from '../../components/chat/context/set-job-scope-context';
import { usePromptSelectionStore } from '../../components/prompt/context/prompt-selection-context';
import { useAuth } from '../../store/auth';
import { useSettings } from '../../store/settings';
import { useShinkaiNodeManager } from '../../store/shinkai-node-manager';

export const showSpotlightWindow = async () => {
  return invoke('show_spotlight_window_app');
};

const EmptyMessage = () => {
  const auth = useAuth((state) => state.auth);

  const { llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });
  const isLocalShinkaiNodeIsUse = useShinkaiNodeManager(
    (state) => state.isInUse,
  );
  const { t } = useTranslation();

  const resetJobScope = useSetJobScope((state) => state.resetJobScope);
  const setPromptSelected = usePromptSelectionStore(
    (state) => state.setPromptSelected,
  );
  const navigate = useNavigate();
  const defaultAgentId = useSettings((state) => state.defaultAgentId);

  const { mutateAsync: createJob } = useCreateJob({
    onError: (error) => {
      toast.error('Failed to send message', {
        description: error.response?.data?.message ?? error.message,
      });
    },
    onSuccess: async (data) => {
      void navigate(
        `/inboxes/${encodeURIComponent(buildInboxIdFromJobId(data.jobId))}`,
      );
    },
  });
  useEffect(() => {
    resetJobScope();
    setPromptSelected(undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onCreateJob = async (message: string) => {
    if (!auth) return;
    await createJob({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      llmProvider: defaultAgentId,
      content: message,
      isHidden: false,
      chatConfig: {
        stream: DEFAULT_CHAT_CONFIG.stream,
        custom_prompt: '',
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        top_p: DEFAULT_CHAT_CONFIG.top_p,
        top_k: DEFAULT_CHAT_CONFIG.top_k,
        use_tools: DEFAULT_CHAT_CONFIG.use_tools,
        thinking: DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
        web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
      },
    });
  };

  return (
    <div
      className="flex size-full items-center justify-center p-6"
      style={{ contain: 'strict' }}
    >
      <motion.div
        animate={{ opacity: 1 }}
        className="flex flex-col items-center gap-6 pt-10 text-center"
        exit={{ opacity: 0 }}
        initial={{ opacity: 0 }}
        transition={{ duration: 0.3 }}
      >
        <div>
          <span aria-hidden={true} className="mb-4 text-4xl">
            🤖
          </span>

          <h1 className="mb-2 text-3xl font-bold text-white">
            {t('chat.emptyStateTitle')}
          </h1>
          <p className="text-text-secondary text-sm">
            {t('chat.emptyStateDescription')}
          </p>
        </div>
        <div className="container grid grid-cols-2 items-center gap-3">
          <Badge
            className="cursor-pointer justify-between rounded-full py-2 text-left font-normal text-balance normal-case transition-colors"
            onClick={() => showSpotlightWindow()}
            variant="outline"
          >
            Quick Ask Spotlight
            <ArrowUpRight className="ml-2 h-3.5 w-3.5" />
          </Badge>
          {[
            {
              text: 'Search in DuckDuckGo',
              prompt: 'Search in DuckDuckGo for: ',
            },
            {
              text: 'Summarize a Youtube video',
              prompt: 'Summarize a Youtube video: ',
            },
          ].map((suggestion) => (
            <Badge
              className="cursor-pointer justify-between rounded-full py-2 text-left font-normal text-balance normal-case transition-colors"
              key={suggestion.text}
              onClick={() => {
                setPromptSelected({
                  name: '',
                  prompt: suggestion.prompt,
                  is_enabled: true,
                  is_favorite: false,
                  is_system: true,
                  version: '1',
                  useTools: true,
                  rowid: 0,
                });
                const element = document.querySelector(
                  '#chat-input',
                ) as HTMLDivElement;
                if (element) {
                  element?.focus?.();
                }
              }}
              variant="outline"
            >
              {suggestion.text}
              <ArrowUpRight className="ml-2 h-3.5 w-3.5" />
            </Badge>
          ))}
          <Badge
            className="cursor-pointer justify-between rounded-full py-2 text-left font-normal text-balance normal-case transition-colors"
            onClick={() => onCreateJob('Tell me about the Roman Empire')}
            variant="outline"
          >
            Tell me about the Roman Empire
            <ArrowUpRight className="ml-2 h-3.5 w-3.5" />
          </Badge>
        </div>

        <div className="mt-4">
          {llmProviders.length === 0 ? (
            <Link
              className={buttonVariants({
                variant: 'default',
              })}
              to={isLocalShinkaiNodeIsUse ? '/install-ai-models' : '/add-ai'}
            >
              <span>{t('llmProviders.add')}</span>
            </Link>
          ) : null}
        </div>
      </motion.div>
    </div>
  );
};
export default EmptyMessage;
