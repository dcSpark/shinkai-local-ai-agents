import z from 'zod';

export const agentFormSchema = z.object({
  name: z.string(),
  llmProviderId: z.string(),
  uiDescription: z.string(),
  storage_path: z.string(),
  knowledge: z.array(z.string()),
  tools: z.array(z.string()),
  tools_config_override: z.record(z.record(z.any())).optional(),
  debugMode: z.boolean(),
  config: z
    .object({
      custom_prompt: z.string(),
      custom_system_prompt: z.string(),
      temperature: z.number(),
      top_k: z.number(),
      top_p: z.number(),
      use_tools: z.boolean(),
      stream: z.boolean(),
      thinking: z.boolean(),
      reasoning_effort: z.enum(['low', 'medium', 'high']).optional(),
      web_search_enabled: z.boolean().optional(),
      other_model_params: z.record(z.string()),
    })
    .nullable(),
  cronExpression: z.string().optional(),
  aiPrompt: z.string().optional(),
});

export type AgentFormValues = z.infer<typeof agentFormSchema>;
