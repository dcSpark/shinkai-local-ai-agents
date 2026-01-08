import {
  type UseMutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type CreateJobInput, type CreateJobOutput } from './types';
import { createJob } from '.';

type Options = UseMutationOptions<CreateJobOutput, APIError, CreateJobInput>;

// Note: For new chats, the optimistic assistant message is added by a useEffect
// in useChatConversationWithOptimisticUpdates after the conversation is fetched.
// This handles the case where the user navigates to the chat and the server
// returns the user message but the assistant hasn't started responding yet.
export const useCreateJob = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createJob,
    ...options,
    onSuccess: async (response, variables, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_INBOXES_WITH_PAGINATION],
      });

      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_JOB_SCOPE],
      });
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_VR_FILES],
      });
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_AGENT_INBOXES, variables.llmProvider],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, context);
      }
    },
  });
};
