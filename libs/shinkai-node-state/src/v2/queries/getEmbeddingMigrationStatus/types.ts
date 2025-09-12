export type GetEmbeddingMigrationStatusInput = {
  nodeAddress: string;
  token: string;
};

export type GetEmbeddingMigrationStatusOutput = {
  current_embedding_model: string;
  migration_in_progress: boolean;
  ready: boolean;
  status: string;
};
