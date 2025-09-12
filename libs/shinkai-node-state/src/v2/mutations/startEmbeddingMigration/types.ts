export type StartEmbeddingMigrationInput = {
  nodeAddress: string;
  token: string;
  force: boolean;
  embedding_model: string;
};

export type StartEmbeddingMigrationOutput = {
  message: string;
};
