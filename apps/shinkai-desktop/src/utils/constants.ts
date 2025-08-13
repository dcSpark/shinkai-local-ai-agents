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
      .map((model) => [model.name, { forceEnabled: true, reasoningLevel: false }])
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

  // Shinkai Backend
  'shinkai-backend:free_text_inference': { forceEnabled: true, reasoningLevel: true },
  'shinkai-backend:standard_text_inference': { forceEnabled: true, reasoningLevel: true },
} as const;
