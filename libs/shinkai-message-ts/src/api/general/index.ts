import { httpClient } from '../../http-client';
import { urlJoin } from '../../utils/url-join';
import {
  type CheckHealthResponse,
  type DockerStatusResponse,
  type EmbeddingMigrationRequest,
  type EmbeddingMigrationResponse,
  type GetEmbeddingMigrationStatusResponse,
  type GetNodeStorageLocationResponse,
  type GetPreferencesResponse,
  type GetShinkaiFreeModelQuotaResponse,
  type InitialRegistrationRequest,
  type InitialRegistrationResponse,
  type SetPreferencesRequest,
  type SetPreferencesResponse,
} from './types';

export const checkHealth = async (nodeAddress: string) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/health_check'),
    { responseType: 'json' },
  );
  return response.data as CheckHealthResponse;
};

export const getDockerStatus = async (nodeAddress: string) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/docker_status'),
    { responseType: 'json' },
  );
  return response.data as DockerStatusResponse;
};

export const getNodeStorageLocation = async (
  nodeAddress: string,
  token: string,
): Promise<GetNodeStorageLocationResponse> => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/storage_location'),
    { responseType: 'json', headers: { Authorization: `Bearer ${token}` } },
  );
  return response.data;
};

export const updateNodeName = async (
  nodeAddress: string,
  token: string,
  newNodeName: string,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/change_node_name'),
    { new_name: newNodeName },
    { headers: { Authorization: `Bearer ${token}` } },
  );
  return response.data;
};

export const initialRegistration = async (
  nodeAddress: string,
  payload: InitialRegistrationRequest,
): Promise<InitialRegistrationResponse> => {
  const healthResponse = await checkHealth(nodeAddress);
  const { status, is_pristine } = healthResponse;
  if (status !== 'ok') {
    return { status: 'error' };
  }
  if (!is_pristine) {
    return { status: 'non-pristine' };
  }

  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/initial_registration'),
    payload,
    { responseType: 'json' },
  );
  const data = response.data;
  return { status: 'success', data };
};

export const getShinkaiFreeModelQuota = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/shinkai_backend_quota'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: {
        model: 'FREE_TEXT_INFERENCE',
      },
      responseType: 'json',
    },
  );
  return response.data as GetShinkaiFreeModelQuotaResponse;
};

export const setPreferences = async (
  nodeAddress: string,
  bearerToken: string,
  payload: SetPreferencesRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/set_preferences'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as SetPreferencesResponse;
};

export const getPreferences = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/get_preferences'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as GetPreferencesResponse;
};

export const startEmbeddingMigration = async (
  nodeAddress: string,
  bearerToken: string,
  payload: EmbeddingMigrationRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/embedding_migration'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as EmbeddingMigrationResponse;
};

export const getEmbeddingMigrationStatus = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/embedding_migration'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as GetEmbeddingMigrationStatusResponse;
};
