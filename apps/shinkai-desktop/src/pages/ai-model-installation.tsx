import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { ModelPrefix } from '@shinkai_network/shinkai-message-ts/api/jobs/index';
import { Models } from '@shinkai_network/shinkai-node-state/lib/utils/models';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import {
  Button,
  buttonVariants,
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';

import { cn } from '@shinkai_network/shinkai-ui/utils';
import { QueryClientProvider } from '@tanstack/react-query';
import { Plus, PlusIcon, Sparkles } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { ModelProvider } from '../components/ais/constants';
import ProviderIcon from '../components/ais/provider-icon';
import { ResourcesBanner } from '../components/hardware-capabilities/resources-banner';
import { OllamaModels } from '../components/shinkai-node-manager/ollama-models';
import { useURLQueryParams } from '../hooks/use-url-query-params';
import { shinkaiNodeQueryClient } from '../lib/shinkai-node-manager/shinkai-node-manager-client';
import { useAuth } from '../store/auth';
import { FixedHeaderLayout } from './layout/simple-layout';

const cloudProviders = [
  {
    id: Models.OpenAI,
    prefix: ModelPrefix.OpenAI,
    name: 'OpenAI',
    icon: <ProviderIcon provider={ModelProvider.OpenAI} />,
    models: [],
    description:
      'OpenAI is an AI research lab that aims to ensure that artificial general intelligence benefits all of humanity.',
  },
  {
    id: Models.TogetherComputer,
    prefix: ModelPrefix.TogetherAI,
    name: 'Together AI',
    icon: <ProviderIcon provider={ModelProvider.TogetherAI} />,
    models: [],
    description:
      'Together AI focus on efficient AI compute with high-performance inference and training on open ModelPrefix.',
  },
  {
    id: Models.Gemini,
    prefix: ModelPrefix.Gemini,
    name: 'Gemini',
    description:
      ' A state-of-the-art large language model offering advanced reasoning, multimodal capabilities, and extended context handling.',
    hasInstructions: true,
    docUrl: 'https://ai.google.dev/api',
    icon: <ProviderIcon provider={ModelProvider.Gemini} />,
  },
  {
    id: Models.Groq,
    prefix: ModelPrefix.Groq,
    name: 'Groq',
    description:
      'A hardware-accelerated inference solution optimized for low-latency and high-throughput deployments.',
    hasInstructions: true,
    docUrl: 'https://console.groq.com/docs/overview',
    icon: <ProviderIcon provider={ModelProvider.Groq} />,
  },
  {
    id: Models.OpenRouter,
    prefix: ModelPrefix.OpenRouter,
    name: 'OpenRouter',
    description:
      'A scalable orchestration layer that intelligently routes queries across multiple AI models, optimizing performance and cost.',
    docUrl: 'https://openrouter.ai/docs/quick-start',
    icon: <ProviderIcon provider={ModelProvider.OpenRouter} />,
  },
  {
    id: Models.Exo,
    prefix: ModelPrefix.Exo,
    name: 'Exo',
    description:
      'A specialized model platform focused on precise information extraction, efficient summarization, and reliable knowledge retrieval.',
    icon: <ProviderIcon provider={ModelProvider.Exo} />,
  },
  {
    id: Models.Claude,
    prefix: ModelPrefix.Claude,
    name: 'Claude',
    models: [],
    docUrl: 'https://docs.anthropic.com/en/docs/intro-to-claude',
    icon: <ProviderIcon provider={ModelProvider.Claude} />,
    description:
      'Claude is a powerful AI model that can be used to generate text, images, and code.',
  },
  {
    id: Models.Grok,
    prefix: ModelPrefix.Grok,
    name: 'Grok',
    icon: <ProviderIcon provider={ModelProvider.Grok} />,
    models: [],
    description:
      'Grok is a family of Large Language Models (LLMs) developed by xAI. Grok is a maximally truth-seeking AI that provides insightful, unfiltered truths about the universe.',
    docUrl: 'https://docs.x.ai/docs/overview',
  },
];
const AIModelInstallation = () => {
  const [selectedProvider, setSelectedProvider] = useState<'local' | 'cloud'>(
    'local',
  );

  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const [showAllOllamaModels, setShowAllOllamaModels] = useState(false);

  const query = useURLQueryParams();

  useEffect(() => {
    const provider = query.get('provider');
    if (provider) {
      setSelectedProvider(provider as 'local' | 'cloud');
    }
  }, [query]);

  const { data: llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const providerModels = useMemo(() => {
    if (!llmProviders) return {};

    return llmProviders.reduce(
      (acc, provider) => {
        const [providerName] = provider.model.split(':');
        if (providerName === ModelPrefix.Ollama) {
          return acc;
        }
        if (!acc[providerName]) {
          acc[providerName] = [];
        }
        acc[providerName].push(provider.model.split(':')[1]);
        return acc;
      },
      {} as Record<string, string[]>,
    );
  }, [llmProviders]);

  const cloudProvidersWithInstalledModels = useMemo(() => {
    return cloudProviders.map((provider) => ({
      ...provider,
      models: providerModels[provider.prefix] || [],
    }));
  }, [providerModels]);

  const navigate = useNavigate();
  return (
    <Tabs
      className="flex h-screen w-full flex-col overflow-hidden"
      onValueChange={(value) => setSelectedProvider(value as 'local' | 'cloud')}
      value={selectedProvider}
    >
      <QueryClientProvider client={shinkaiNodeQueryClient}>
        <FixedHeaderLayout
          className="relative flex h-full w-full max-w-6xl flex-col gap-2 px-4"
          rightElement={
            <TabsList className="bg-bg-dark flex h-10 w-fit items-center gap-2 rounded-full p-1.5">
              <TabsTrigger
                className={cn(
                  'flex gap-2 rounded-full px-4 py-1.5 text-sm font-medium transition-colors',
                  'data-[state=active]:bg-bg-quaternary data-[state=active]:text-text-default',
                  'data-[state=inactive]:text-text-tertiary data-[state=inactive]:bg-transparent',
                  'focus-visible:outline-hidden',
                )}
                value="local"
              >
                <svg fill="none" height="16" viewBox="0 0 16 16" width="16">
                  <g clipPath="url(#clip0_3706_1161)">
                    <path
                      d="M3.99967 12.0002H5.33301M7.99967 12.0002H11.9997M4.66634 1.3335H11.333C12.0694 1.3335 12.6663 1.93045 12.6663 2.66683V5.3335C12.6663 6.06988 12.0694 6.66683 11.333 6.66683H4.66634C3.92996 6.66683 3.33301 6.06988 3.33301 5.3335V2.66683C3.33301 1.93045 3.92996 1.3335 4.66634 1.3335ZM2.66634 9.3335H13.333C14.0694 9.3335 14.6663 9.93045 14.6663 10.6668V13.3335C14.6663 14.0699 14.0694 14.6668 13.333 14.6668H2.66634C1.92996 14.6668 1.33301 14.0699 1.33301 13.3335V10.6668C1.33301 9.93045 1.92996 9.3335 2.66634 9.3335Z"
                      stroke="currentColor"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </g>
                  <defs>
                    <clipPath id="clip0_3706_1161">
                      <rect fill="currentColor" height="16" width="16" />
                    </clipPath>
                  </defs>
                </svg>
                Local AI
              </TabsTrigger>
              <TabsTrigger
                className={cn(
                  'flex gap-2 rounded-full px-4 py-1.5 text-sm font-medium transition-colors',
                  'data-[state=active]:bg-bg-quaternary data-[state=active]:text-text-default',
                  'data-[state=inactive]:text-text-tertiary data-[state=inactive]:bg-transparent',
                  'focus-visible:outline-hidden',
                )}
                value="cloud"
              >
                <svg fill="none" height="16" viewBox="0 0 16 16" width="16">
                  <path
                    d="M11.6671 12.6668H6.00043C5.13501 12.6666 4.28669 12.4257 3.55028 11.9711C2.81387 11.5165 2.21838 10.8661 1.83035 10.0926C1.44231 9.31902 1.27703 8.4528 1.35296 7.59071C1.42889 6.72862 1.74305 5.90463 2.26033 5.21082C2.77761 4.517 3.47763 3.98069 4.28217 3.66182C5.0867 3.34295 5.96406 3.25408 6.8162 3.40514C7.66834 3.5562 8.46168 3.94124 9.10759 4.51723C9.75349 5.09323 10.2265 5.83748 10.4738 6.66683H11.6671C12.4627 6.66683 13.2258 6.9829 13.7884 7.54551C14.351 8.10812 14.6671 8.87118 14.6671 9.66683C14.6671 10.4625 14.351 11.2255 13.7884 11.7882C13.2258 12.3508 12.4627 12.6668 11.6671 12.6668Z"
                    stroke="currentColor"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
                Cloud AI
              </TabsTrigger>
            </TabsList>
          }
          title={t('llmProviders.localAI.installTitle')}
        >
          <ResourcesBanner />
          <TabsContent className="h-full" value="local">
            <div className="flex items-center justify-between gap-10 space-y-2 pb-4">
              <div className="flex-1">
                <h1 className="font-clash text-lg font-medium">Local AI</h1>
                <p className="text-text-secondary text-sm">
                  Local AI operates directly on your device, providing immediate
                  responses and strict data privacy <br />
                  with no internet required. Ideal for consistent, secure AI
                  access anywhere.
                </p>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  className={cn('shrink-0')}
                  onClick={() => setShowAllOllamaModels(!showAllOllamaModels)}
                  size="sm"
                  variant="outline"
                >
                  <Sparkles className="h-4 w-4" />
                  <span className="capitalize">
                    {showAllOllamaModels
                      ? t('shinkaiNode.models.labels.showRecommended')
                      : t('shinkaiNode.models.labels.showAll')}
                  </span>
                </Button>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      className={cn('shrink-0')}
                      onClick={() => navigate('/add-ai?aiProvider=ollama')}
                      size="sm"
                      variant="outline"
                    >
                      <PlusIcon className="h-4 w-4" />
                      Add Ollama Model
                    </Button>
                  </TooltipTrigger>
                  <TooltipPortal>
                    <TooltipContent align="end" side="bottom">
                      Already installed a model with Ollama on your machine?
                      Connect it here without reinstalling.
                    </TooltipContent>
                  </TooltipPortal>
                </Tooltip>
              </div>
            </div>
            <OllamaModels
              parentSetShowAllOllamaModels={setShowAllOllamaModels}
              parentShowAllOllamaModels={showAllOllamaModels}
            />
          </TabsContent>
          <TabsContent className="h-full" value="cloud">
            <div className="flex items-center justify-between gap-10 space-y-2 pb-4">
              <div className="max-w-3xl">
                <h1 className="font-clash text-lg font-medium">Cloud AI</h1>
                <p className="text-text-secondary text-sm">
                  Cloud AI leverages remote servers for powerful computational
                  capabilities, offering scalability and real-time updates.
                  Access cutting-edge AI features with an active internet
                  connection.
                </p>
              </div>
              <Button
                className={cn('shrink-0')}
                onClick={() => navigate('/add-ai')}
                size="sm"
                variant="outline"
              >
                <Plus className="h-4 w-4" />
                <span className="capitalize">Add Custom AI Model</span>
              </Button>
            </div>
            <div className="grid grid-cols-4 gap-4">
              {cloudProvidersWithInstalledModels.map((model) => (
                <Card
                  className="flex h-[235px] flex-col justify-between"
                  key={model.name}
                >
                  <div className="space-y-3">
                    <CardHeader className="px-4 pt-4 pb-0">
                      <div className="flex items-center gap-2">
                        <div
                          className={`flex h-6 w-6 items-center justify-center`}
                        >
                          {model.icon}
                        </div>
                        <h3 className="font-clash text-lg font-semibold">
                          {model.name}
                        </h3>
                      </div>
                    </CardHeader>
                    <CardContent className="space-y-4 px-4">
                      {model.models && model.models.length > 0 ? (
                        <div>
                          <p className="text-text-secondary mb-2 text-xs uppercase">
                            Available Models
                          </p>
                          <ul className="space-y-1">
                            {model.models.map((item) => (
                              <li
                                className="text-text-secondary flex items-center gap-2 truncate text-xs"
                                key={item}
                              >
                                <span>✓</span>
                                {item}
                              </li>
                            ))}
                          </ul>
                        </div>
                      ) : (
                        <p className="text-text-secondary text-xs">
                          {model.description}
                        </p>
                      )}

                      {model.hasInstructions && (
                        <a
                          className="w-full text-xs underline"
                          href={model.docUrl}
                          rel="noreferrer"
                          target="_blank"
                        >
                          Instructions
                        </a>
                      )}
                    </CardContent>
                  </div>

                  <CardFooter className="px-4 pb-4">
                    <Link
                      className={cn(
                        buttonVariants({
                          size: 'sm',
                          variant: 'outline',
                        }),
                      )}
                      state={{
                        type: 'cloud',
                      }}
                      to={`/add-ai?aiProvider=${model.id}`}
                    >
                      <Plus className="mr-2 h-4 w-4" />
                      Add AI
                    </Link>
                  </CardFooter>
                </Card>
              ))}
            </div>
          </TabsContent>
        </FixedHeaderLayout>
      </QueryClientProvider>
    </Tabs>
  );
};

export default AIModelInstallation;
