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

export const OLLAMA_MODELS_WITH_THINKING_SUPPORT = Array.from(
  new Set(
    OLLAMA_MODELS_REPOSITORY.filter((model) => model.thinking)
      .map((model) => model.name)
      .map((name) => name.toLowerCase().replace(/-/g, '_')),
  ),
);
