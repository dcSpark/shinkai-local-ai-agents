import { type QueryObserverOptions, useQuery } from '@tanstack/react-query';

import { DEFAULT_CHAT_CONFIG, FunctionKeyV2 } from '../../constants';
import { type GetChatConfigInput, type GetChatConfigOutput } from './types';
import { getChatConfig } from './index';

export type UseGetChatConfig = [
  FunctionKeyV2.GET_CHAT_CONFIG,
  GetChatConfigInput,
];

type Options = QueryObserverOptions<
  GetChatConfigOutput,
  Error,
  GetChatConfigOutput,
  GetChatConfigOutput,
  UseGetChatConfig
>;

export const useGetChatConfig = (
  input: GetChatConfigInput,
  options?: Omit<Options, 'queryKey' | 'queryFn'>,
) => {
  const response = useQuery({
    queryKey: [FunctionKeyV2.GET_CHAT_CONFIG, input],
    queryFn: () => getChatConfig(input),
    select: (data) => {
      return {
        custom_prompt: data.custom_prompt,
        temperature: data.temperature ?? DEFAULT_CHAT_CONFIG.temperature,
        seed: data.seed ?? DEFAULT_CHAT_CONFIG.seed,
        top_k: data.top_k ?? DEFAULT_CHAT_CONFIG.top_k,
        top_p: data.top_p ?? DEFAULT_CHAT_CONFIG.top_p,
        stream: data.stream ?? DEFAULT_CHAT_CONFIG.stream,
        use_tools: data.use_tools ?? DEFAULT_CHAT_CONFIG.use_tools,
        thinking: data.thinking ?? DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: data.reasoning_effort ?? DEFAULT_CHAT_CONFIG.reasoning_effort,
      };
    },
    ...options,
  });
  return response;
};
