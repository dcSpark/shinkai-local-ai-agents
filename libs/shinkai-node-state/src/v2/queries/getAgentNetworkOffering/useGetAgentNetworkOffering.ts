import { type QueryObserverOptions, useQuery } from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import {
  type GetAgentNetworkOfferingInput,
  type GetAgentNetworkOfferingOutput,
} from './types';
import { getAgentNetworkOffering } from '.';

export type UseGetAgentNetworkOffering = [
  FunctionKeyV2.GET_AGENT_NETWORK_OFFERING,
  GetAgentNetworkOfferingInput,
];

type Options = QueryObserverOptions<
  GetAgentNetworkOfferingOutput,
  Error,
  GetAgentNetworkOfferingOutput,
  GetAgentNetworkOfferingOutput,
  UseGetAgentNetworkOffering
>;

export const useGetAgentNetworkOffering = (
  input: GetAgentNetworkOfferingInput,
  options?: Omit<Options, 'queryKey' | 'queryFn'>,
) => {
  const response = useQuery({
    queryKey: [FunctionKeyV2.GET_AGENT_NETWORK_OFFERING, input],
    queryFn: () => getAgentNetworkOffering(input),
    ...options,
  });
  return response;
};
