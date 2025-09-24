import { type UseMutationOptions, useMutation } from '@tanstack/react-query';

import { type APIError } from '../../types';
import {
  type SetCommonToolsetConfigInput,
  type SetCommonToolsetConfigOutput,
} from './types';
import { setCommonToolsetConfig } from '.';

type Options = UseMutationOptions<
  SetCommonToolsetConfigOutput,
  APIError,
  SetCommonToolsetConfigInput
>;

export const useSetCommonToolsetConfig = (options?: Options) => {
  const { onSuccess: onOptionsSuccess, ...restOptions } = options || {};
  return useMutation({
    mutationFn: setCommonToolsetConfig,
    onSuccess: (...args) => {
      if (onOptionsSuccess) {
        onOptionsSuccess(...args);
      }
    },
    ...restOptions,
  });
};
