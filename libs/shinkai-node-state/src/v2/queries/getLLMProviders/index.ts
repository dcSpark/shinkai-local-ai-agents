import { getLLMProviders as getLLMProvidersAPI } from '@shinkai_network/shinkai-message-ts/api/jobs/index';

import { type GetLLMProvidersInput } from './types';

const EMBEDDING_MODEL = 'ollama:embeddinggemma:300m';

export const getLLMProviders = async ({
  nodeAddress,
  token,
}: GetLLMProvidersInput) => {
  const result = await getLLMProvidersAPI(nodeAddress, token);

  const filteredProviders = result
    .filter((provider) => provider.model !== EMBEDDING_MODEL)
    .filter((provider) => provider.model !== 'shinkai-backend:code_generator');

  return filteredProviders;
};
