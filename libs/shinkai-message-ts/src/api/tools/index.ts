import { httpClient } from '../../http-client';
import { urlJoin } from '../../utils/url-join';
import {
  type AddToolRequest,
  type AddToolRequestRequest,
  type AddToolRequestResponse,
  type CopyToolAssetsRequest,
  type CopyToolAssetsResponse,
  type CreatePromptRequest,
  type CreatePromptResponse,
  type CreateToolCodeRequest,
  type CreateToolCodeResponse,
  type CreateToolMetadataRequest,
  type CreateToolMetadataResponse,
  type DeletePromptRequest,
  type DuplicateToolRequest,
  type DuplicateToolResponse,
  type ExecuteToolCodeRequest,
  type ExecuteToolCodeResponse,
  type ExportToolRequest,
  type ExportToolResponse,
  type GetAllPromptsResponse,
  type GetAllToolAssetsResponse,
  type GetPlaygroundToolRequest,
  type GetPlaygroundToolResponse,
  type GetPlaygroundToolsResponse,
  type GetShinkaiFileProtocolRequest,
  type GetShinkaiFileProtocolResponse,
  type GetToolPlaygroundMetadataRequest,
  type GetToolPlaygroundMetadataResponse,
  type GetToolProtocolsResponse,
  type GetToolResponse,
  type GetToolsFromToolsetResponse,
  type GetToolsRequest,
  type GetToolsResponse,
  type GetToolStoreDetailsRequest,
  type GetToolStoreDetailsResponse,
  type ImportToolRequest,
  type ImportToolResponse,
  type ImportToolZipRequest,
  type ImportToolZipResponse,
  type OpenToolInCodeEditorRequest,
  type OpenToolInCodeEditorResponse,
  type PayInvoiceRequest,
  type RejectInvoiceRequest,
  type PublishToolRequest,
  type PublishToolResponse,
  type RemovePlaygroundToolRequest,
  type RemoveToolRequest,
  type RemoveToolRequestRequest,
  type SaveToolCodeRequest,
  type SaveToolCodeResponse,
  type SearchPromptsResponse,
  type SetCommonToolsetConfigRequest,
  type SetCommonToolsetConfigResponse,
  type SetOAuthTokenRequest,
  type SetOAuthTokenResponse,
  type ToggleEnableToolRequest,
  type ToggleEnableToolResponse,
  type UndoToolImplementationRequest,
  type UndoToolImplementationResponse,
  type UpdatePromptRequest,
  type UpdateToolCodeImplementationRequest,
  type UpdateToolCodeImplementationResponse,
  type UpdateToolRequest,
  type UpdateToolResponse,
  type GetToolsWithOfferingsResponse,
  type SetToolOfferingRequest,
  type SetToolOfferingResponse,
  type GetInstalledNetworkToolsResponse,
  type AddNetworkToolRequest,
  type GetNetworkAgentsResponse,
  type RemoveToolOfferingRequest,
} from './types';

export const createTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: AddToolRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/add_shinkai_tool'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as UpdateToolResponse;
};

export const getTool = async (
  nodeAddress: string,
  bearerToken: string,
  toolKey: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/get_shinkai_tool'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { tool_name: toolKey, serialize_config: true },
      responseType: 'json',
    },
  );
  return response.data as GetToolResponse;
};

export const getTools = async (
  nodeAddress: string,
  bearerToken: string,
  payload: GetToolsRequest,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/list_all_shinkai_tools'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { category: payload.category },
      responseType: 'json',
    },
  );
  return response.data as GetToolsResponse;
};

export const searchTools = async (
  nodeAddress: string,
  bearerToken: string,
  query: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/search_shinkai_tool'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { query },
      responseType: 'json',
    },
  );
  return response.data as GetToolsResponse;
};

export const getToolsFromToolset = async (
  nodeAddress: string,
  bearerToken: string,
  tool_set_key: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/tools_from_toolset'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { tool_set_key },
      responseType: 'json',
    },
  );
  return response.data as GetToolsFromToolsetResponse;
};

export const setCommonToolsetConfig = async (
  nodeAddress: string,
  bearerToken: string,
  payload: SetCommonToolsetConfigRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/set_common_toolset_config'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as SetCommonToolsetConfigResponse;
};

export const updateTool = async (
  nodeAddress: string,
  bearerToken: string,
  toolKey: string,
  payload: UpdateToolRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/set_shinkai_tool'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { tool_name: encodeURI(toolKey) },
      responseType: 'json',
    },
  );
  return response.data as UpdateToolResponse;
};

export const toggleEnableTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: ToggleEnableToolRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/set_tool_enabled'),
    { tool_router_key: payload.tool_router_key, enabled: payload.enabled },
    { headers: { Authorization: `Bearer ${bearerToken}` } },
  );
  return response.data as ToggleEnableToolResponse;
};

export const payInvoice = async (
  nodeAddress: string,
  bearerToken: string,
  payload: PayInvoiceRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/pay_invoice'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const rejectInvoice = async (
  nodeAddress: string,
  bearerToken: string,
  payload: RejectInvoiceRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/reject_invoice'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};
export const getAllPrompts = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/get_all_custom_prompts'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as GetAllPromptsResponse;
};

export const searchPrompt = async (
  nodeAddress: string,
  bearerToken: string,
  query: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/search_custom_prompts'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { query },
      responseType: 'json',
    },
  );
  return response.data as SearchPromptsResponse;
};

export const createPrompt = async (
  nodeAddress: string,
  bearerToken: string,
  payload: CreatePromptRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/add_custom_prompt'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as CreatePromptResponse;
};
export const updatePrompt = async (
  nodeAddress: string,
  bearerToken: string,
  payload: UpdatePromptRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/update_custom_prompt'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const removePrompt = async (
  nodeAddress: string,
  bearerToken: string,
  payload: DeletePromptRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/delete_custom_prompt'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const toolImplementation = async (
  nodeAddress: string,
  bearerToken: string,
  payload: CreateToolCodeRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/tool_implementation'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as CreateToolCodeResponse;
};
export const toolMetadataImplementation = async (
  nodeAddress: string,
  bearerToken: string,
  payload: CreateToolMetadataRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/tool_metadata_implementation'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as CreateToolMetadataResponse;
};

export const getToolPlaygroundMetadata = async (
  nodeAddress: string,
  bearerToken: string,
  payload: GetToolPlaygroundMetadataRequest,
) => {
  const url = urlJoin(nodeAddress, '/v2/get_shinkai_tool_metadata');
  const params = new URLSearchParams();
  params.set('tool_router_key', payload.tool_router_key);
  const response = await httpClient.get(url, {
    headers: { Authorization: `Bearer ${bearerToken}` },
    params,
    responseType: 'json',
  });
  return response.data as GetToolPlaygroundMetadataResponse;
};

export const executeToolCode = async (
  nodeAddress: string,
  bearerToken: string,
  payload: ExecuteToolCodeRequest,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/code_execution'),
    payload,
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
        'x-shinkai-app-id': xShinkaiAppId,
        'x-shinkai-tool-id': xShinkaiToolId,
      },
      responseType: 'json',
    },
  );
  return response.data as ExecuteToolCodeResponse;
};

export const saveToolCode = async (
  nodeAddress: string,
  bearerToken: string,
  payload: SaveToolCodeRequest,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
  xShinkaiOriginalToolRouterKey?: string,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/set_playground_tool'),
    payload,
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
        'x-shinkai-app-id': xShinkaiAppId,
        'x-shinkai-tool-id': xShinkaiToolId,
        ...(xShinkaiOriginalToolRouterKey && {
          'x-shinkai-original-tool-router-key': xShinkaiOriginalToolRouterKey,
        }),
      },
      responseType: 'json',
    },
  );
  return response.data as SaveToolCodeResponse;
};

export const getPlaygroundTools = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/list_playground_tools'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as GetPlaygroundToolsResponse;
};

export const getPlaygroundTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: GetPlaygroundToolRequest,
  xShinkaiOriginalToolRouterKey?: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/get_playground_tool'),
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
        ...(xShinkaiOriginalToolRouterKey && {
          'x-shinkai-original-tool-router-key': xShinkaiOriginalToolRouterKey,
          'x-shinkai-copy-metadata': 'true',
        }),
      },
      params: { tool_key: payload.tool_key },
      responseType: 'json',
    },
  );
  return response.data as GetPlaygroundToolResponse;
};

export const restoreToolConversation = async (
  nodeAddress: string,
  bearerToken: string,
  payload: UndoToolImplementationRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/tool_implementation_undo_to'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as UndoToolImplementationResponse;
};

export const removeTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: RemoveToolRequest,
) => {
  const response = await httpClient.delete(
    urlJoin(nodeAddress, '/v2/remove_tool'),
    {
      params: { tool_key: payload.tool_key },
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const removePlaygroundTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: RemovePlaygroundToolRequest,
) => {
  const response = await httpClient.delete(
    urlJoin(nodeAddress, '/v2/remove_playground_tool'),
    {
      params: { tool_key: payload.tool_key },
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const updateToolCodeImplementation = async (
  nodeAddress: string,
  bearerToken: string,
  payload: UpdateToolCodeImplementationRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/tool_implementation_code_update'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as UpdateToolCodeImplementationResponse;
};

export const openToolInCodeEditor = async (
  nodeAddress: string,
  bearerToken: string,
  payload: OpenToolInCodeEditorRequest,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
  xShinkaiLLMProvider: string,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/tools_standalone_playground'),
    payload,
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
        'x-shinkai-app-id': xShinkaiAppId,
        'x-shinkai-tool-id': xShinkaiToolId,
        'x-shinkai-llm-provider': xShinkaiLLMProvider,
      },
      responseType: 'json',
    },
  );
  return response.data as OpenToolInCodeEditorResponse;
};
export const duplicateTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: DuplicateToolRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/duplicate_tool'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );

  return response.data as DuplicateToolResponse;
};

export const importTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: ImportToolRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/import_tool'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as ImportToolResponse;
};

export const exportTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: ExportToolRequest,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/export_tool'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { tool_key_path: payload.toolKey },
      responseType: 'blob',
    },
  );
  return response.data as ExportToolResponse;
};

export const getShinkaiFileProtocol = async (
  nodeAddress: string,
  bearerToken: string,
  payload: GetShinkaiFileProtocolRequest,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/resolve_shinkai_file_protocol'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      params: { file: payload.file },
      responseType: 'blob',
    },
  );
  return response.data as GetShinkaiFileProtocolResponse;
};

export const setOAuthToken = async (
  nodeAddress: string,
  bearerToken: string,
  payload: SetOAuthTokenRequest,
): Promise<SetOAuthTokenResponse> => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/set_oauth_token'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as SetOAuthTokenResponse;
};

export const getAllToolAssets = async (
  nodeAddress: string,
  bearerToken: string,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/list_tool_asset'),
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
        'x-shinkai-app-id': xShinkaiAppId,
        'x-shinkai-tool-id': xShinkaiToolId,
      },
      responseType: 'json',
    },
  );
  return response.data as GetAllToolAssetsResponse;
};

export const uploadAssetTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: AddToolRequestRequest,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
) => {
  const fileData = await payload.file.arrayBuffer();

  const formData = new FormData();
  formData.append('file_name', payload.filename);
  formData.append('file', new Blob([fileData]));

  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/tool_asset'),
    formData,
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
        'x-shinkai-app-id': xShinkaiAppId,
        'x-shinkai-tool-id': xShinkaiToolId,
      },
      responseType: 'json',
    },
  );
  return response.data as AddToolRequestResponse;
};

export const uploadPlaygroundToolFile = async (
  nodeAddress: string,
  bearerToken: string,
  payload: AddToolRequestRequest,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
) => {
  const fileData = await payload.file.arrayBuffer();

  const formData = new FormData();
  formData.append('file_name', payload.filename);
  formData.append('file', new Blob([fileData]));

  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/playground_file'),
    formData,
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
        'x-shinkai-app-id': xShinkaiAppId,
        'x-shinkai-tool-id': xShinkaiToolId,
      },
      responseType: 'json',
    },
  );
  return response.data as AddToolRequestResponse;
};

export const uploadAssetsToTool = async (
  nodeAddress: string,
  bearerToken: string,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
  files: File[],
) => {
  for (const file of files) {
    await uploadAssetTool(
      nodeAddress,
      bearerToken,
      {
        filename: encodeURIComponent(file.name),
        file,
      },
      xShinkaiAppId,
      xShinkaiToolId,
    );
  }
  return { success: true };
};

export const uploadPlaygroundToolFiles = async (
  nodeAddress: string,
  bearerToken: string,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
  files: File[],
) => {
  const fileContent: Record<string, string> = {};
  for (const file of files) {
    const response = await uploadPlaygroundToolFile(
      nodeAddress,
      bearerToken,
      {
        filename: encodeURIComponent(file.name),
        file,
      },
      xShinkaiAppId,
      xShinkaiToolId,
    );
    fileContent[file.name] = response.file_path;
  }
  return { success: true, fileContent };
};

export const removeToolAsset = async (
  nodeAddress: string,
  bearerToken: string,
  payload: RemoveToolRequestRequest,
  xShinkaiAppId: string,
  xShinkaiToolId: string,
) => {
  const response = await httpClient.delete(
    urlJoin(nodeAddress, '/v2/tool_asset'),
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
        'x-shinkai-app-id': xShinkaiAppId,
        'x-shinkai-tool-id': xShinkaiToolId,
      },
      params: { file_name: payload.filename },
      responseType: 'json',
    },
  );
  return response.data;
};

export const enableAllTools = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/enable_all_tools'),
    {},
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const disableAllTools = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/disable_all_tools'),
    {},
    { headers: { Authorization: `Bearer ${bearerToken}` } },
  );
  return response.data;
};
export const publishTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: PublishToolRequest,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/publish_tool'),
    {
      params: { tool_key_path: payload.tool_key_path },
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as PublishToolResponse;
};

export const getToolStoreDetails = async (
  nodeAddress: string,
  bearerToken: string,
  payload: GetToolStoreDetailsRequest,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, `/v2/tool_store_proxy/${payload.tool_router_key}`),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as GetToolStoreDetailsResponse;
};

export const importToolZip = async (
  nodeAddress: string,
  bearerToken: string,
  payload: ImportToolZipRequest,
) => {
  const fileData = await payload.file.arrayBuffer();

  const formData = new FormData();
  formData.append('file', new Blob([fileData]));

  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/import_tool_zip'),
    formData,
    {
      headers: {
        Authorization: `Bearer ${bearerToken}`,
      },
      responseType: 'json',
    },
  );
  return response.data as ImportToolZipResponse;
};

export const copyToolAssets = async (
  nodeAddress: string,
  bearerToken: string,
  payload: CopyToolAssetsRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/copy_tool_assets'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as CopyToolAssetsResponse;
};

export const getToolsWithOfferings = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/get_tools_with_offerings'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as GetToolsWithOfferingsResponse;
};

export const setToolOffering = async (
  nodeAddress: string,
  bearerToken: string,
  payload: SetToolOfferingRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/set_tool_offering'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as SetToolOfferingResponse;
};

export const removeToolOffering = async (
  nodeAddress: string,
  bearerToken: string,
  payload: RemoveToolOfferingRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/remove_tool_offering'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const setToolMcpEnabled = async (
  nodeAddress: string,
  bearerToken: string,
  toolRouterKey: string,
  mcpEnabled: boolean,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/set_tool_mcp_enabled'),
    {
      tool_router_key: toolRouterKey,
      mcp_enabled: mcpEnabled,
    },
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const getToolProtocols = async () => {
  const response = await httpClient.get(
    'https://api.shinkai.com/kb/index.json',
    {
      responseType: 'json',
    },
  );
  return response.data as GetToolProtocolsResponse;
};

export const getInstalledNetworkTools = async (
  nodeAddress: string,
  bearerToken: string,
) => {
  const response = await httpClient.get(
    urlJoin(nodeAddress, '/v2/list_all_network_shinkai_tools'),
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data as GetInstalledNetworkToolsResponse;
};

export const addNetworkTool = async (
  nodeAddress: string,
  bearerToken: string,
  payload: AddNetworkToolRequest,
) => {
  const response = await httpClient.post(
    urlJoin(nodeAddress, '/v2/add_network_agent'),
    payload,
    {
      headers: { Authorization: `Bearer ${bearerToken}` },
      responseType: 'json',
    },
  );
  return response.data;
};

export const getNetworkAgents = async () => {
  const response = await httpClient.get(
    'https://api.shinkai.com/store/dapps/offerings',
    { responseType: 'json' },
  );
  return response.data as GetNetworkAgentsResponse;
};
