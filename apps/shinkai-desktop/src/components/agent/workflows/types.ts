import { type ReactNode } from 'react';
import { type Edge, type Node, type XYPosition } from 'reactflow';

export type LLMOption = 'Gemini' | 'ChatGPT 5' | 'Claude';

export type WorkflowNodeType =
  | 'agent'
  | 'tool'
  | 'conditional'
  | 'note'
  | 'start'
  | 'guardrail';

export type ConditionalLLMConfig = {
  model: string;
  prompt: string;
};

export type ConditionalConfig =
  | {
      type: 'expression';
      expression: string;
      llmDraft?: ConditionalLLMConfig;
    }
  | {
      type: 'llm';
      llm: ConditionalLLMConfig;
      expressionDraft?: string;
    };

export type NodePaletteType = Exclude<WorkflowNodeType, 'start'>;

export type WorkflowNodeData = {
  label: string;
  description?: string;
  nodeType: WorkflowNodeType;
  agentConfig?: {
    name: string;
    llm: LLMOption;
    instructions?: string;
  };
  conditionalConfig?: ConditionalConfig;
};

export type WorkflowNode = Node<WorkflowNodeData>;

export type WorkflowEdge = Edge;

export type WorkflowState = {
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
};

export type WorkflowPaletteItem = {
  type: NodePaletteType;
  label: string;
  description: string;
  icon: ReactNode;
  accentColor: string;
  accentSoftColor: string;
};

export type NodeAccent = Pick<
  WorkflowPaletteItem,
  'icon' | 'accentColor' | 'accentSoftColor'
>;

export type WorkflowPayloadEdge = {
  source: string;
  target: string;
  forward_condition: {
    label: string | null;
    type: 'unconditional' | 'conditional' | 'expression';
    expression?: {
      type: 'cel';
      source: string;
    };
  };
  backward_condition: unknown | null;
};

export type WorkflowPayloadNode = {
  type: WorkflowNodeType;
  position: XYPosition;
  label?: string;
  llm?: LLMOption;
  instructions?: string;
  edge_order?: string[];
  content?: string;
  description?: string;
  config?: ConditionalConfig;
  [key: string]: unknown;
};

export type WorkflowPayload = {
  edges: Record<string, WorkflowPayloadEdge>;
  nodes: Record<string, WorkflowPayloadNode>;
};
