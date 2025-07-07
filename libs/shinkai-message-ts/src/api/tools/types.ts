import { type JobMessage } from '../jobs/types';

export type ShinkaiToolHeader = {
  author: string;
  config?: ToolConfigBase[];
  description: string;
  enabled: boolean;
  formatted_tool_summary_for_ui: string;
  name: string;
  tool_router_key: string;
  tool_type: ShinkaiToolType;
  version: string;
  input_args: {
    properties: Record<string, { description: string; type: string }>;
  };
  mcp_enabled: boolean | null;
};

export type ToolConfig = {
  type: 'object';
  properties: Record<string, any>;
  required: string[];
};

export type ToolArgument = {
  type: 'object';
  properties: Record<string, any>;
  required: string[];
};

export type ToolConfigBase = {
  BasicConfig: {
    description: string;
    key_name: string;
    key_value: string | null;
    required: boolean;
    type: string | null;
  };
};

type Embedding = {
  id: string;
  vector: number[];
};

type JSToolResult = {
  result_type: string;
  properties: Record<string, string>;
  required: string[];
};

export type OAuth = {
  name: string;
  authorizationUrl: string;
  tokenUrl: string;
  clientId: string;
  clientSecret: string;
  redirectUrl: string;
  version: '1.0' | '2.0';
  responseType: 'code';
  scopes: string[];
  pkceType: 'plain' | undefined;
  refreshToken: 'false' | 'true' | undefined;
};

export type DenoShinkaiTool = {
  toolkit_name: string;
  tool_router_key?: string;
  name: string;
  author: string;
  js_code: string;
  config: ToolConfigBase;
  configurations: ToolConfig;
  configFormData: Record<string, any>;
  description: string;
  keywords: string[];
  oauth: OAuth[];
  input_args: ToolArgument;
  config_set: boolean;
  activated: boolean;
  embedding?: Embedding;
  result: Record<string, any>;
  tool_set: string;
};
export type PythonShinkaiTool = {
  activated: boolean;
  author: string;
  keywords: string[];
  input_args: ToolArgument;
  description: string;
  config: ToolConfigBase;
  configurations: ToolConfig;
  configFormData: Record<string, any>;
  name: string;
  oauth: OAuth[];
  py_code: string;
  output_arg: {
    json: string;
  };
  result: JSToolResult;
  tool_set: string;
};
export type RustShinkaiTool = {
  description: string;
  input_args: ToolArgument;
  name: string;
  output_arg: {
    json: string;
  };
  tool_embedding?: Embedding;
  tool_router_key: string;
};

export type NetworkShinkaiTool = {
  activated: boolean;
  author: string;
  config?: ToolConfigBase;
  // configurations?: ToolConfig;
  // configFormData: Record<string, any>;
  description: string;
  // embedding: null | string;
  input_args: ToolArgument;
  name: string;
  output_arg: {
    json: string;
  };
  provider: string;
  restrictions?: string;
  toolkit_name?: string;
  usage_type: ToolUsageType;
  version: string;
  tool_router_key: string;
};

export type AgentShinkaiTool = {
  agent_id: string;
  name: string;
  full_identity_name: string;
  ui_description: string;
  description: string; // Alias for ui_description to match other tool types
  knowledge: string[];
  tools: string[];
  debug_mode: boolean;
  llm_provider_id: string;
  author?: string;
};

export type McpServerTool = {
  id: number;
  name: string;
  description: string;
  tool_router_key?: string;
  mcp_server_ref: string;
  mcp_server_tool: string;
  activated: boolean;
  author?: string;
  config?: Record<string, unknown>;
  embedding?: number[];
  input_args: ToolArgument;
  keywords?: string[];
  mcp_enabled: boolean;
  mcp_server_url: string;
  output_arg: {
    json: string;
  };
  result?: {
    type: string;
    properties: Record<string, unknown>;
    required: string[];
  };
  tool_set: string;
  version: string;
};

export type ShinkaiToolType =
  | 'Deno'
  | 'Python'
  | 'Rust'
  | 'Network'
  | 'Agent'
  | 'MCPServer';

export type ShinkaiTool =
  | DenoShinkaiTool
  | PythonShinkaiTool
  | RustShinkaiTool
  | NetworkShinkaiTool
  | AgentShinkaiTool
  | McpServerTool;

export type GetToolResponse = {
  content: [ShinkaiTool, boolean];
  type: ShinkaiToolType;
};

export type GetToolsCategory =
  | 'downloaded'
  | 'default'
  | 'system'
  | 'my_tools'
  | 'mcp_servers'
  | 'all';
export type GetToolsRequest = {
  category?: GetToolsCategory;
};
export type GetToolsResponse = ShinkaiToolHeader[];
export type GetToolsSearchResponse = [ShinkaiToolHeader, number][];

export type GetToolsFromToolsetResponse = {
  type: ShinkaiToolType;
  content: [ShinkaiToolHeader, boolean];
}[];

export type SetCommonToolsetConfigRequest = {
  tool_set_key: string;
  value: Record<string, unknown>;
};

export type SetCommonToolsetConfigResponse = {
  updated_tool_keys: string[];
};
export type AddToolRequest = {
  type: ShinkaiToolType;
  content: [ShinkaiTool, boolean];
};

export type UpdateToolRequest = {
  type: ShinkaiToolType;
  content: [ShinkaiTool, boolean];
};
export type UpdateToolResponse = {
  type: ShinkaiToolType;
  content: [ShinkaiTool, boolean];
};

export type ToggleEnableToolRequest = {
  tool_router_key: string;
  enabled: boolean;
};

export type ToggleEnableToolResponse = {
  message: string;
  status: string;
};

// TODO: fix types after backend is ready

export type ToolPrice =
  | 'Free'
  | { DirectDelegation: string }
  | {
      Payment: Array<{
        // amount?: string;
        // maxAmountRequired?: string;
        // asset: {
        //   asset_id: string;
        //   contract_address: string;
        //   decimals: number;
        //   network_id: string;
        // };
        asset: string;
        description: string;
        extra: {
          name: string;
          version: string;
        };
        maxAmountRequired?: string;
        maxTimeoutSeconds?: number;
        network?: string;
        payTo?: string;
        resource?: string;
        scheme?: string;
        mimeType?: string;
        outputSchema?: Record<string, any>;
      }>;
    };

export type ToolUsageType = {
  PerUse: ToolPrice;
  // Downloadable: ToolPrice; // TODO: add this back in when backend is ready
};
export type PaymentTool = {
  toolKey: string;
  description: string;
  usageType: ToolUsageType;
};

export type PayInvoiceRequest = {
  invoice_id: string;
  data_for_tool: any;
};

export type PayInvoiceResponse = any;

export type RejectInvoiceRequest = {
  invoice_id: string;
  reason?: string | null;
};

export type RejectInvoiceResponse = any;

export type Prompt = {
  rowid: number;
  name: string;
  prompt: string;
  is_system: boolean;
  is_enabled: boolean;
  version: string;
  is_favorite: boolean;
  useTools?: boolean; // flag for prompt templates
};
export type CreatePrompt = Omit<Prompt, 'rowid'>;

export type GetAllPromptsResponse = Prompt[];
export type SearchPromptsResponse = Prompt[];
export type CreatePromptRequest = CreatePrompt;
export type CreatePromptResponse = Prompt;
export type UpdatePromptRequest = Prompt;
export type DeletePromptRequest = {
  prompt_name: string;
};

export enum CodeLanguage {
  Typescript = 'Typescript',
  Python = 'Python',
  Agent = 'Agent',
  MCPServer = 'MCPServer',
}

export type CreateToolCodeRequest = {
  message: JobMessage;
  language: CodeLanguage;
  raw?: boolean;
  tools: string[];
};

export type CreateToolCodeResponse = {
  message_id: string;
  parent_message_id?: string;
  inbox: string;
  scheduled_time: string;
};

export type CreateToolMetadataRequest = {
  language: CodeLanguage;
  job_id: string;
  tools: string[];
  x_shinkai_tool_id?: string;
};

export type CreateToolMetadataResponse = {
  job_id: string;
};

export enum DynamicToolType {
  DenoDynamic = 'denodynamic',
  PythonDynamic = 'pythondynamic',
  AgentDynamic = 'agentdynamic',
  MCPServerDynamic = 'mcpserverdynamic',
}

export type ExecuteToolCodeRequest = {
  tool_type: DynamicToolType;
  parameters: Record<string, any>;
  code: string;
  extra_config?: Record<string, any>;
  llm_provider: string;
  tools: string[];
  mounts?: string[];
};

export type ExecuteToolCodeResponse = Record<string, any>;

export type SaveToolCodeRequest = {
  tool_router_key?: string;
  metadata: Record<string, any>;
  job_id: string;
  job_id_history?: string[];
  code: string;
  language: CodeLanguage;
  assets: string[];
};

export type ToolMetadata = {
  name: string;
  description: string;
  author: string;
  keywords: string[];
  version?: string;
  configurations: {
    type: 'object';
    properties: Record<string, any>;
    required: string[];
  };
  parameters: {
    type: 'object';
    properties: Record<string, any>;
    required: string[];
  };
  result: {
    type: 'object';
    properties: Record<string, any>;
    required: string[];
  };
  tools: string[];
};

export type SaveToolCodeResponse = {
  shinkai_tool: {
    type: 'Deno';
    content: [ShinkaiTool, boolean];
  };
  metadata: {
    metadata: ToolMetadata;
    tool_router_key: string;
    job_id: string;
    code: string;
  };
};

export type PlaygroundTool = {
  metadata: ToolMetadata;
  tool_router_key: string;
  job_id: string;
  job_id_history: string[];
  code: string;
  language: string;
};
export type GetPlaygroundToolsResponse = PlaygroundTool[];
export type GetPlaygroundToolRequest = { tool_key: string };
export type GetPlaygroundToolResponse = PlaygroundTool;
export type UndoToolImplementationRequest = {
  message_hash: string;
  job_id: string;
};
export type UndoToolImplementationResponse = {
  message: string;
  status: string;
};
export type UpdateToolCodeImplementationRequest = {
  code: string;
  job_id: string;
};
export type UpdateToolCodeImplementationResponse = {
  message: string;
  status: string;
};
export type OpenToolInCodeEditorRequest = {
  language: CodeLanguage;
};
export type OpenToolInCodeEditorResponse = {
  message: string;
  status: 'success' | 'error';
  files: Record<string, string>;
  playground_path: string;
};
export type DuplicateToolRequest = {
  tool_key_path: string;
};
export type DuplicateToolResponse = {
  job_id: string;
  tool_router_key: string;
  version: string;
};
export type RemovePlaygroundToolRequest = {
  tool_key: string;
};

export type RemoveToolRequest = {
  tool_key: string;
};

export type ImportToolRequest = {
  url: string;
};

export type ImportToolResponse = {
  message: string;
  status: string;
  tool: GetToolResponse;
  tool_key: string;
};

export type ExportToolRequest = {
  toolKey: string;
};

export type ExportToolResponse = Blob;
export type GetShinkaiFileProtocolRequest = { file: string };
export type GetShinkaiFileProtocolResponse = Blob;

export type GetAllToolAssetsResponse = string[];
export type AddToolRequestRequest = {
  filename: string;
  file: File;
};
export type AddToolRequestResponse = {
  file: number;
  file_name: string;
  file_path: string;
  message: string;
  status: string;
};
export type RemoveToolRequestRequest = {
  filename: string;
};
export type SetOAuthTokenRequest = {
  code: string;
  state: string;
};

export type SetOAuthTokenResponse = {
  message: string;
  status: string;
};
export type EnableAllToolsResponse = {
  [toolKey: string]: {
    activated: boolean;
  };
};
export type DisableAllToolsResponse = {
  [toolKey: string]: {
    activated: boolean;
  };
};

export type PublishToolRequest = {
  tool_key_path: string;
};
export type PublishToolResponse = {
  message: string;
  response: {
    message: string;
    revisionId: string;
  };
  status: string;
  tool_key: string;
};

export type GetToolStoreDetailsRequest = {
  tool_router_key: string;
};
export type GetToolStoreDetailsResponse = {
  assets: {
    bannerUrl: string;
    iconUrl: string;
  };
  product: {
    product: {
      author: string;
      banner_url: string;
      categoryId: string;
      createdAt: string;
      description: string;
      downloads: number;
      featured: boolean;
      icon_url: string;
      id: string;
      isActive: boolean;
      keywords: string[];
      name: string;
      operating_system: string[];
      price_usd: number;
      routerKey: string;
      runner: string;
      stripeProductId: string;
      toolLanguage: string;
      tool_set: string;
      type: string;
      category: {
        id: string;
        name: string;
        description: string;
        examples: string;
      };
    };
    versions: {
      file: string;
      hash: string;
      id: string;
      node_version: string;
      version: string;
    }[];
  };
};
export type ImportToolZipRequest = {
  file: File;
};
export type ImportToolZipResponse = {
  message: string;
  status: string;
  tool_key: string;
};

export type CopyToolAssetsRequest = {
  is_first_playground: boolean;
  first_path: string;
  second_path: string;
  is_second_playground: boolean;
};
export type CopyToolAssetsResponse = {
  message: string;
};

export type ToolOffering = {
  meta_description: string;
  tool_key: string;
  usage_type: ToolUsageType;
};

export type NetworkToolNode = {
  id: string;
  peerId: string;
  identity: string;
  online: boolean;
  lastActivity: string;
  connected: string;
  lastPingRtt: number;
  bytesTx: number;
  pingFailures: number;
};

export type NetworkToolWithOffering = {
  network_tool: NetworkShinkaiTool;
  tool_offering: ToolOffering;
  node: NetworkToolNode;
};

export type GetToolsWithOfferingsResponse = NetworkToolWithOffering[];

export type GetToolProtocolsResponse = {
  created: string;
  supported: {
    name: string;
    icon: string;
    documentationURL: string;
  }[];
};

export type GetToolPlaygroundMetadataRequest = {
  tool_router_key: string;
};

export type GetToolPlaygroundMetadataResponse = ToolMetadata | null;

export type SetToolOfferingRequest = {
  tool_offering: ToolOffering;
};

export type SetToolOfferingResponse = {
  message: string;
};
export type RemoveToolOfferingRequest = {
  tool_key_name: string;
};

export type GetInstalledNetworkToolsResponse = ShinkaiToolHeader[];

export type AddNetworkToolRequest = {
  assets: any[];
  tool: {
    type: 'Network';
    content: [NetworkShinkaiTool, boolean];
  };
};

export type GetNetworkAgentsResponse = NetworkToolWithOffering[];
