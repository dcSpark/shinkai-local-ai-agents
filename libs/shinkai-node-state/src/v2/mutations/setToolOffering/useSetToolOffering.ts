import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type SetToolOfferingInput, type SetToolOfferingOutput } from './types';
import { setToolOffering } from './index';

export const useSetToolOffering = (
  options?: UseMutationOptions<
    SetToolOfferingOutput,
    APIError,
    SetToolOfferingInput
  >,
) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: setToolOffering,
    ...options,
    onSuccess: async (...args) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_TOOLS_WITH_OFFERINGS],
      });
      if (options?.onSuccess) {
        options.onSuccess(...args);
      }
    },
  });
};
