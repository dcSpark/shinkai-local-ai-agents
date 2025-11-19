import OLLAMA_MODELS_REPOSITORY from '../lib/shinkai-node-manager/ollama-models-repository.json';

export const SHINKAI_DOCS_URL = 'https://docs.shinkai.com';

export const SHINKAI_TUTORIALS = {
  'add-ai':
    'https://pub-0f9ce9e619a7477aa6e92197a3ec4a1e.r2.dev/assets/agents.mp4',
  'file-explorer':
    'https://pub-5b510bf382c744c093bdd1619d2d6d3e.r2.dev/file-explorer.mp4',
  'scheduled-tasks':
    'https://pub-5b510bf382c744c093bdd1619d2d6d3e.r2.dev/scheduled-tasks.mp4',
  'shinkai-tools':
    'https://pub-0f9ce9e619a7477aa6e92197a3ec4a1e.r2.dev/assets/tools.mp4',
} as const;

export const MODELS_WITH_THINKING_SUPPORT = {
  // Ollama models with thinking support
  ...Object.fromEntries(
    OLLAMA_MODELS_REPOSITORY.filter((model) => model.thinking)
      .map((model) => model.tags)
      .flat()
      .map((tags) => [`ollama:${tags.name}`, { forceEnabled: true, reasoningLevel: false }])
  ),
  
  // Claude models
  'claude:claude-opus-4-1-20250805': { forceEnabled: false, reasoningLevel: true },
  'claude:claude-opus-4-1': { forceEnabled: false, reasoningLevel: true },
  'claude:claude-opus-4-20250514': { forceEnabled: false, reasoningLevel: true },
  'claude:claude-opus-4-0': { forceEnabled: false, reasoningLevel: true },
  'claude:claude-sonnet-4-20250514': { forceEnabled: false, reasoningLevel: true },
  'claude:claude-sonnet-4-0': { forceEnabled: false, reasoningLevel: true },
  'claude:claude-3-7-sonnet-20250219': { forceEnabled: false, reasoningLevel: true },
  'claude:claude-3-7-sonnet-latest': { forceEnabled: false, reasoningLevel: true },
  
  // DeepSeek models
  'deepseek:deepseek-reasoner': { forceEnabled: true, reasoningLevel: false },

  // Groq models
  'groq:openai/gpt-oss-20b': { forceEnabled: false, reasoningLevel: true },
  'groq:openai/gpt-oss-120b': { forceEnabled: false, reasoningLevel: true },
  'groq:qwen/qwen3-32b': { forceEnabled: false, reasoningLevel: false },
  'groq:deepseek-r1-distill-llama-70b': { forceEnabled: false, reasoningLevel: false },

  // OpenAI models
  'openai:gpt-5': { forceEnabled: false, reasoningLevel: true },
  'openai:gpt-5-mini': { forceEnabled: false, reasoningLevel: true },
  'openai:gpt-5-nano': { forceEnabled: false, reasoningLevel: true },
  'openai:gpt-5.1': { forceEnabled: false, reasoningLevel: true },
  'openai:o1': { forceEnabled: false, reasoningLevel: true },
  'openai:o1-mini': { forceEnabled: false, reasoningLevel: true },
  'openai:o3': { forceEnabled: false, reasoningLevel: true },
  'openai:o3-mini': { forceEnabled: false, reasoningLevel: true },
  'openai:o4': { forceEnabled: false, reasoningLevel: true },
  'openai:o4-mini': { forceEnabled: false, reasoningLevel: true },  

  // Gemini models
  'gemini:gemini-3-pro-preview': { forceEnabled: false, reasoningLevel: true },
  'gemini:gemini-2.5-pro': { forceEnabled: false, reasoningLevel: true },
  'gemini:gemini-2.5-flash': { forceEnabled: false, reasoningLevel: true },
  'gemini:gemini-2.5-flash-preview-05-20': { forceEnabled: false, reasoningLevel: true },
  'gemini:gemini-2.5-flash-lite': { forceEnabled: false, reasoningLevel: true },
  'gemini:gemini-2.5-flash-lite-preview-06-17': { forceEnabled: false, reasoningLevel: true },
  'gemini:gemini-2.0-flash-exp': { forceEnabled: false, reasoningLevel: true },

  // Shinkai Backend
  'shinkai-backend:free_text_inference': { forceEnabled: false, reasoningLevel: true },
  'shinkai-backend:standard_text_inference': { forceEnabled: false, reasoningLevel: true },
  'shinkai-backend:premium_text_inference': { forceEnabled: false, reasoningLevel: true },
} as const;
