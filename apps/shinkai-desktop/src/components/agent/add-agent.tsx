import { zodResolver } from '@hookform/resolvers/zod';
import { DEFAULT_CHAT_CONFIG } from '@shinkai_network/shinkai-node-state/v2/constants';
import { useCreateAgent } from '@shinkai_network/shinkai-node-state/v2/mutations/createAgent/useCreateAgent';
import {
  Button,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormMessage,
  Input,
} from '@shinkai_network/shinkai-ui';

import { useForm } from 'react-hook-form';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';
import { useAnalytics } from '../../lib/posthog-provider';
import { useAuth } from '../../store/auth';
import { useSettings } from '../../store/settings';
import { agentFormSchema, type AgentFormValues } from './agent-schema';

function AddAgentPage() {
  const auth = useAuth((state) => state.auth);
  const defaultAgentId = useSettings((state) => state.defaultAgentId);
  const navigate = useNavigate();

  const { captureAnalyticEvent } = useAnalytics();

  const { mutateAsync: createAgent, isPending: isCreating } = useCreateAgent({
    onError: (error) => {
      toast.error('Failed to create agent', {
        description: error.response?.data?.message ?? error.message,
      });
    },
    onSuccess: async (_, variables) => {
      await navigate(`/agents/edit/${variables.agent.agent_id}`);
      captureAnalyticEvent('Agent Created', undefined);

      // if (options?.openChat) {
      //   await navigate(`/agents/edit/${agentIdToUse}?openChat=true`);
      // } else {
      //   await navigate('/agents');
      // }
    },
  });

  const form = useForm<AgentFormValues>({
    resolver: zodResolver(agentFormSchema),
    defaultValues: {
      name: '',
      uiDescription: '',
      storage_path: '',
      knowledge: [],
      tools: [],
      tools_config_override: {},
      debugMode: false,
      config: {
        stream: DEFAULT_CHAT_CONFIG.stream,
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        top_p: DEFAULT_CHAT_CONFIG.top_p,
        top_k: DEFAULT_CHAT_CONFIG.top_k,
        custom_prompt: '',
        custom_system_prompt: '',
        other_model_params: {},
        use_tools: true,
        thinking: DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
        web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
      },
      llmProviderId: defaultAgentId,
      cronExpression: '',
      aiPrompt: '',
    },
  });

  const submit = async (values: AgentFormValues) => {
    if (!auth) return;
    const agentIdToUse = values.name
      .replace(/[^a-zA-Z0-9_]/g, '_')
      .toLowerCase();

    const agentData = {
      agent_id: agentIdToUse,
      full_identity_name: `${auth.shinkai_identity}/main/agent/${agentIdToUse}`,
      name: values.name,
      llm_provider_id: defaultAgentId ?? '',
      ui_description: '',
      storage_path: '',
      knowledge: [],
      tools: [],
      tools_config_override: {},
      debug_mode: false,
      config: {
        custom_prompt: '',
        custom_system_prompt: '',
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        top_k: DEFAULT_CHAT_CONFIG.top_k,
        top_p: DEFAULT_CHAT_CONFIG.top_p,
        use_tools: DEFAULT_CHAT_CONFIG.use_tools,
        stream: DEFAULT_CHAT_CONFIG.stream,
        thinking: DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
        web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
        other_model_params: {},
      },
      cron_tasks: [],
      aiPrompt: '',
    };

    await createAgent({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      agent: agentData,
      cronExpression: '',
    });
  };
  console.log(form.formState.errors);

  return (
    <div className="flex h-full w-full flex-col items-center justify-center">
      <Form {...form}>
        <form
          className="w-full max-w-md space-y-4"
          onSubmit={form.handleSubmit((values) => submit(values))}
        >
          <div className="space-y-1">
            <h1 className="font-clash text-2xl font-medium">New Agent</h1>
            <p className="text-text-secondary text-sm">
              Give your agent a name that describes its role or function
            </p>
          </div>
          <FormField
            control={form.control}
            name="name"
            render={({ field }) => (
              <FormItem>
                <FormControl>
                  <div className="relative">
                    <Input
                      className="h-10 w-full pt-2 pr-14"
                      {...field}
                      placeholder="eg: Sales Assistant, Customer Support, etc."
                      maxLength={50}
                    />
                    <span className="text-text-secondary absolute top-1/2 right-3 -translate-y-1/2 text-xs">
                      {field.value.length}/50
                    </span>
                  </div>
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
          <Button
            type="submit"
            className="w-full"
            disabled={isCreating}
            isLoading={isCreating}
          >
            Create Agent
          </Button>
        </form>
      </Form>
    </div>
  );
}

export default AddAgentPage;
