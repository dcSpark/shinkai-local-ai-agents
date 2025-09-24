import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type ExportToolInput, type ExportToolOutput } from './types';
import { exportTool } from '.';

type Options = UseMutationOptions<ExportToolOutput, APIError, ExportToolInput>;

export const useExportTool = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: exportTool,
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
