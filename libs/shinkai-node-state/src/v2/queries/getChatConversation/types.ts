import {
  type Token,
  type ToolArgs,
  type ToolStatusType,
} from '@shinkai_network/shinkai-message-ts/api/general/types';
import { type ProviderDetails } from '@shinkai_network/shinkai-message-ts/api/jobs/types';
import { type InfiniteData } from '@tanstack/react-query';
import { type ReactNode } from 'react';

export type GetChatConversationInput = Token & {
  nodeAddress: string;
  inboxId: string;
  count?: number;
  lastKey?: string;
  shinkaiIdentity: string;
  profile: string;
  enabled?: boolean;
  refetchIntervalEnabled?: boolean;
};

export type Artifact = {
  identifier: string;
  type: string;
  title: string;
  code: string;
  language?: string;
};

export enum FileTypeSupported {
  Image = 'image',
  Video = 'video',
  Audio = 'audio',
  Text = 'text',
  Document = 'document',
  Error = 'error',
  Unknown = 'unknown',
  Html = 'html',
  Csv = 'csv',
  SqliteDatabase = 'sqlite-database',
}

export type ToolCall = {
  toolRouterKey: string;
  name: string;
  args: ToolArgs;
  result?: string;
  status?: ToolStatusType; // TODO: remove
  isError?: boolean;
  generatedFiles?: Attachment[];
};

export type Attachment = {
  id: string;
  path: string;
  name: string;
  extension: string;
  size?: number;
  blob?: Blob;
  type: FileTypeSupported;
  mimeType: string;
  url?: string;
  content?: string;
  error?: string;
};

export type TextStatus =
  | {
      type: 'running';
    }
  | {
      type: 'requires-action';
      reason: 'tool-calls';
    }
  | {
      type: 'complete';
      reason: 'stop' | 'unknown';
    }
  | {
      type: 'incomplete';
      reason:
        | 'cancelled'
        | 'tool-calls'
        | 'length'
        | 'content-filter'
        | 'other'
        | 'error';
      error?: unknown;
    };

type BaseMessage = {
  messageId: string;
  createdAt: string;
  metadata: {
    parentMessageId: string;
    inboxId: string;
    tps?: string;
  };
};
export type TextContentPart = { type: 'text'; text: string };
export type ToolCallContentPart = { type: 'tool-call'; toolCalls: ToolCall[] };
export type UIContentPart = { type: 'ui'; display: ReactNode };

export type UserContentPart = string | TextContentPart | UIContentPart;

export type AssistantContentPart =
  | string
  | TextContentPart
  | ToolCallContentPart
  | UIContentPart;

export type UserMessage = BaseMessage & {
  role: 'user';
  content: string; // UserContentPart
  attachments: Attachment[];
};

export type ReasoningPart = {
  text: string;
  status: TextStatus;
};

export type AssistantMessage = BaseMessage & {
  role: 'assistant';
  content: string; // AssistantContentPart
  status: TextStatus;
  toolCalls: ToolCall[];
  artifacts: Artifact[];
  generatedFiles?: Attachment[];
  reasoning?: ReasoningPart;
  provider?: ProviderDetails;
};

export type FormattedMessage = AssistantMessage | UserMessage;

export type GetChatConversationOutput = FormattedMessage[];

export type ChatConversationInfiniteData =
  InfiniteData<GetChatConversationOutput> & {
    content: FormattedMessage[];
  };
