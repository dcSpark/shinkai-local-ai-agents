import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type ImportToolInput, type ImportToolOutput } from './types';
import { importTool } from '.';

type Options = UseMutationOptions<ImportToolOutput, APIError, ImportToolInput>;

export const useImportTool = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: importTool,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_LIST_TOOLS],
      });
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
