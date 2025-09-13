export type AgentOrLLMProvider = {
  full_identity_name: string;
  id: string;
  model: string;
  name: string;
  description: string;
};

export enum VectorSearchMode {
  FillUpTo25k = 'FillUpTo25k',
  MergeSiblings = 'MergeSiblings',
}

export type ShinkaiPath = string;

export type JobScope = {
  vector_fs_items: ShinkaiPath[];
  vector_fs_folders: ShinkaiPath[];
  vector_search_mode?: VectorSearchMode[];
};

export type CreateJobRequest = {
  llm_provider: string;
  job_creation_info: {
    scope: JobScope;
    associated_ui: 'Playground' | null;
    is_hidden: boolean;
  };
};
export type CreateJobResponse = {
  job_id: string;
};

export type JobMessageRequest = {
  job_message: JobMessage;
};
export type JobMessageResponse = {
  inbox: string;
  message_id: string;
  parent_message_id: string;
  scheduled_time: string;
};

export type GetLastMessagesRequest = {
  inbox_name: string;
  limit: number;
  offset_key?: string;
};
export type GetLastMessagesWithBranchesRequest = {
  inbox_name: string;
  limit: number;
  offset_key?: string;
};
export type NodeApiData = {
  node_message_hash: string;
  node_timestamp: string;
  parent_hash: string;
};

export type JobMessage = {
  job_id: string;
  callback?: null;
  content: string;
  parent: string | null;
  sheet_job_data?: null;
  tool_key?: string;
  metadata?: {
    tps?: string;
    duration_ms: string;
    function_calls: {
      name: string;
      arguments: {
        message: string;
      };
      tool_router_key: string;
      response?: string;
    }[];
  };
  fs_files_paths?: ShinkaiPath[];
  job_filenames?: string[];
  reasoning_content: string | null;
};
export type ChatMessage = {
  job_message: JobMessage;
  sender: string;
  sender_subidentity: string;
  receiver: string;
  receiver_subidentity: string;
  node_api_data: NodeApiData;
  inbox: string;
};

export type GetLastMessagesResponse = ChatMessage[];
export type GetLastMessagesWithBranchesResponse = ChatMessage[][];

export type UpdateLLMProviderInJobRequest = {
  job_id: string;
  new_agent_id: string;
};
export type UpdateLLMProviderInJobResponse = { result: string };

export type CreateFilesInboxResponse = string;

export type AddFileToInboxRequest = {
  file_inbox_name: string;
  filename: string;
  file: File;
};
export type AddFileToInboxResponse = {
  message: string;
  filename: string;
};

export type LLMProvider = {
  id: string;
  full_identity_name: string;
  external_url?: string;
  api_key?: string;
  model: string;
  name: string;
  description?: string;
};

export type GetDownloadFileRequest = {
  path: string;
};
export type GetDownloadFileResponse = string;

export type GetLLMProvidersResponse = LLMProvider[];

export type LLMProviderInterface = {
  OpenAI?: OpenAI;
  TogetherAI?: TogetherAI;
  Ollama?: Ollama;
  Gemini?: Gemini;
  Exo?: Exo;
  Groq?: Groq;
  OpenRouter?: OpenRouter;
  Claude?: Claude;
  DeepSeek?: DeepSeek;
  Grok?: Grok;
} & {
  [model: string]: ModelType;
};

export interface ModelType {
  model_type: string;
}

export interface Ollama {
  model_type: string;
}
export interface Gemini {
  model_type: string;
}

export interface Groq {
  model_type: string;
}

export interface OpenRouter {
  model_type: string;
}

export interface OpenAI {
  model_type: string;
}

export interface TogetherAI {
  model_type: string;
}

export interface Exo {
  model_type: string;
}

export interface Claude {
  model_type: string;
}

export interface DeepSeek {
  model_type: string;
}

export interface Grok {
  model_type: string;
}

export type SerializedLLMProvider = {
  id: string;
  name: string;
  description?: string;
  full_identity_name: string;
  external_url?: string;
  api_key?: string;
  model: LLMProviderInterface;
};
export type AddLLMProviderRequest = SerializedLLMProvider;
export type AddLLMProviderResponse = string;

export type UpdateLLMProviderRequest = SerializedLLMProvider;
export type UpdateLLMProviderResponse = string;

export type RemoveLLMProviderRequest = {
  llm_provider_id: string;
};

export type Inbox = {
  inbox_id: string;
  custom_name: string;
  last_message?: ChatMessage;
  is_finished: boolean;
  job_scope?: JobScope;

  datetime_created: string;
} & ProviderDetails;

export type GetAllInboxesResponse = Inbox[];

export type GetAllAgentInboxesRequest = {
  agent_id: string;
  show_hidden?: boolean;
};

export type GetAllInboxesWithPaginationRequest = {
  limit?: number;
  offset?: string;
  show_hidden?: boolean;
};
export type GetAllInboxesWithPaginationResponse = {
  inboxes: Inbox[];
  hasNextPage: boolean;
};

export type UpdateInboxNameRequest = {
  inbox_name: string;
  custom_name: string;
};
export type UpdateInboxNameResponse = {
  data: null;
  status: string;
};

export type GetFileNamesRequest = {
  inboxName: string;
};

export type GetFileNamesResponse = string[];

export type JobConfig = {
  custom_prompt: string;
  custom_system_prompt?: string;
  temperature?: number;
  seed?: number;
  top_k?: number;
  top_p?: number;
  stream?: boolean;
  use_tools?: boolean;
  thinking?: boolean;
  reasoning_effort?: 'low' | 'medium' | 'high';
  web_search_enabled?: boolean;
  other_model_params?: Record<string, string>;
};
export type GetChatConfigRequest = {
  job_id: string;
};
export type GetChatConfigResponse = JobConfig;
export type UpdateChatConfigRequest = {
  job_id: string;
  config: JobConfig;
};
export type UpdateChatConfigResponse = {
  result: string;
};

export type StopGeneratingLLMRequest = string;
export type StopGeneratingLLMResponse = { status: string };

export type KillJobRequest = {
  conversation_inbox_name: string;
};
export type KillJobResponse = { status: string };

export type GetJobScopeRequest = {
  jobId: string;
};
export type GetJobScopeResponse = JobScope;

export type UpdateJobScopeRequest = {
  job_id: string;
  job_scope: JobScope;
};
export type UpdateJobScopeResponse = string;

export type RetryMessageRequest = {
  message_id: string;
  inbox_name: string;
};

export type RemoveJobRequest = {
  job_id: string;
};
export type AddFileToJobRequest = {
  job_id: string;
  filename: string;
  file: File;
};
export type AddFileToJobResponse = string;
export type GetJobFolderNameRequest = {
  job_id: string;
};
export type GetJobFolderNameResponse = {
  folder_name: string;
};

export type GetProviderFromJobRequest = {
  job_id: string;
};

export type ProviderDetails = {
  agent: AgentOrLLMProvider;
  provider_type: 'Agent' | 'LLMProvider';
};
export type GetProviderFromJobResponse = ProviderDetails;

export type ForkJobMessagesRequest = {
  message_id: string;
  job_id: string;
};

export type ForkJobMessagesResponse = {
  job_id: string;
};

export type ExportInboxMessagesFormat = 'csv' | 'json' | 'txt';

export type ExportMessagesFromInboxRequest = {
  inbox_name: string;
  format: ExportInboxMessagesFormat;
};

export type ExportMessagesFromInboxResponse = Blob;

export type GetMessageTracesRequest = {
  message_id: string;
};

export type MessageTrace = {
  id: number;
  parent_message_id: string;
  inbox_name: string;
  datetime: string;
  trace_name: string;
  trace_info: Record<string, any>;
};

export type GetMessageTracesResponse = MessageTrace[];
