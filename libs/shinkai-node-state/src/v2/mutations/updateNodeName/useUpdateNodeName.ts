import { type UseMutationOptions, useMutation } from '@tanstack/react-query';

import { type APIError } from '../../types';
import { type UpdateNodeNameInput, type UpdateNodeNameOutput } from './types';
import { updateNodeName } from '.';

type Options = UseMutationOptions<
  UpdateNodeNameOutput,
  APIError,
  UpdateNodeNameInput
>;

export const useUpdateNodeName = (options?: Options) => {
  return useMutation({
    mutationFn: updateNodeName,
    ...options,
    onSuccess: (response, variables, onMutateResult, context) => {
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
