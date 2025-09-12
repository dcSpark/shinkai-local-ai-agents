import { startEmbeddingMigration as startEmbeddingMigrationApi } from '@shinkai_network/shinkai-message-ts/api/general/index';

import { type StartEmbeddingMigrationInput } from './types';

export const startEmbeddingMigration = async ({
  nodeAddress,
  token,
  force,
  embedding_model,
}: StartEmbeddingMigrationInput) => {
  const response = await startEmbeddingMigrationApi(nodeAddress, token, {
    force,
    embedding_model,
  });
  return response;
};
