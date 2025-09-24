import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type UpdateQuestsStatusInput,
  type UpdateQuestsStatusOutput,
} from './types';
import { updateQuestsStatus } from './index';

export type UseGetQuestsStatus = [
  FunctionKeyV2.GET_QUESTS_STATUS,
  UpdateQuestsStatusInput,
];

type Options = UseMutationOptions<
  UpdateQuestsStatusOutput,
  APIError,
  UpdateQuestsStatusInput
>;

export const useUpdateQuestsStatus = (
  options?: Omit<Options, 'mutationFn'>,
) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updateQuestsStatus,
    ...options,
    onSuccess: async (data, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_QUESTS_STATUS, variables],
      });
      if (options?.onSuccess) {
        options.onSuccess(data, variables, onMutateResult, context);
      }
    },
  });
};
