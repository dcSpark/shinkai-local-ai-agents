import { httpClient } from '../../http-client';
import { urlJoin } from '../../utils/url-join';
import {
  type CreateAgentRequest,
  type CreateAgentResponse,
  type ExportAgentResponse,
  type GetAgentRequest,
  type GetAgentResponse,
  type GetAgentsResponse,
  type ImportAgentResponse,
  type PublishAgentRequest,
  type PublishAgentResponse,
  type RemoveAgentRequest,
  type RemoveAgentResponse,
  type UpdateAgentRequest,
  type UpdateAgentResponse,
} from './types';

export const createAgent = async (
  nodeAddress: string,
  bearerToken: string,
  payload: CreateAgentRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/add_agent'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as CreateAgentResponse;
};

export const removeAgent = async (
  nodeAddress: string,
  bearerToken: string,
  payload: RemoveAgentRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/remove_agent'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as RemoveAgentResponse;
};

export const updateAgent = async (
  nodeAddress: string,
  bearerToken: string,
  payload: UpdateAgentRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/update_agent'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as UpdateAgentResponse;
};

export const getAgent = async (
  nodeAddress: string,
  bearerToken: string,
  payload: GetAgentRequest,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, `/v2/get_agent/${payload.agentId}`),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as GetAgentResponse;
};

export const getAgents = async (
  nodeAddress: string,
  bearerToken: string,
  categoryFilter?: 'recently_used',
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/get_all_agents'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: {
        filter: categoryFilter,
      },
      responseType: 'json',
    },
  );
  return response.data as GetAgentsResponse;
};

export const exportAgent = async (
  nodeAddress: string,
  bearerToken: string,
  agentId: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/export_agent'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { agent_id: agentId },
      responseType: 'blob',
    },
  );
  return response.data as ExportAgentResponse;
};

export const importAgent = async (
  nodeAddress: string,
  bearerToken: string,
  file: File,
) => {
  const fileData = await file.arrayBuffer();

  const formData = new FormData();
  formData.append('file', new Blob([fileData]));

  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/import_agent_zip'),
    formData,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as ImportAgentResponse;
};

export const importAgentFromUrl = async (
  nodeAddress: string,
  bearerToken: string,
  url: string,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/import_agent'),
    { url },
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as ImportAgentResponse;
};

export const publishAgent = async (
  nodeAddress: string,
  bearerToken: string,
  payload: PublishAgentRequest,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/publish_agent'),
    {
      params: { agent_id: payload.agent_id },
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as PublishAgentResponse;
};
