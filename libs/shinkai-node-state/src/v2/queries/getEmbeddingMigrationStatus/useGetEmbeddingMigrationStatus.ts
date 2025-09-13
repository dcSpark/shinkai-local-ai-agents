import { type QueryObserverOptions, useQuery } from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import {
  type GetEmbeddingMigrationStatusInput,
  type GetEmbeddingMigrationStatusOutput,
} from './types';
import { getEmbeddingMigrationStatus } from '.';

type UseGetEmbeddingMigrationStatus = [
  FunctionKeyV2.GET_EMBEDDING_MIGRATION_STATUS,
  GetEmbeddingMigrationStatusInput,
];

type Options = QueryObserverOptions<
  GetEmbeddingMigrationStatusOutput,
  Error,
  GetEmbeddingMigrationStatusOutput,
  GetEmbeddingMigrationStatusOutput,
  UseGetEmbeddingMigrationStatus
>;

export const useGetEmbeddingMigrationStatus = (
  input: GetEmbeddingMigrationStatusInput,
  options?: Omit<Options, 'queryKey' | 'queryFn'>,
) => {
  const response = useQuery({
    queryKey: [FunctionKeyV2.GET_EMBEDDING_MIGRATION_STATUS, input],
    queryFn: async () => getEmbeddingMigrationStatus(input),
    ...options,
  });
  return response;
};
