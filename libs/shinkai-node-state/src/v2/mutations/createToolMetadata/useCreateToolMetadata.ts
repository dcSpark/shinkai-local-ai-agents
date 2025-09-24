import { useMutation, type UseMutationOptions } from '@tanstack/react-query';

import { type APIError } from '../../types';
import {
  type CreateToolMetadataInput,
  type CreateToolMetadataOutput,
} from './types';
import { createToolMetadata } from '.';

type Options = UseMutationOptions<
  CreateToolMetadataOutput,
  APIError,
  CreateToolMetadataInput
>;

export const useCreateToolMetadata = (options?: Options) => {
  return useMutation({
    mutationFn: createToolMetadata,
    ...options,
    onSuccess: (response, variables, onMutateResult, context) => {
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
