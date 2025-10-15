import {
  AgentIcon,
  GuardrailIcon,
  NoteIcon,
  ToolsIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { GitBranch } from 'lucide-react';

import {
  type LLMOption,
  type NodeAccent,
  type NodePaletteType,
  type WorkflowPaletteItem,
  type WorkflowPayload,
  type WorkflowNodeType,
} from './types';

export const LLM_OPTIONS: LLMOption[] = ['Gemini', 'ChatGPT 5', 'Claude'];
export const DEFAULT_LLM: LLMOption = 'Gemini';

export const MIN_NODE_DISTANCE = 220;
export const MAX_SPACING_ITERATIONS = 60;

export const NODE_TYPE_LABELS: Record<WorkflowNodeType, string> = {
  agent: 'Agent',
  tool: 'Tool',
  conditional: 'Conditional',
  note: 'Note',
  start: 'Start',
  guardrail: 'Guardrail',
};

export const PALETTE_ITEMS: WorkflowPaletteItem[] = [
  {
    type: 'agent',
    label: 'Agent',
    description: 'Call an AI model with custom instructions.',
    icon: <AgentIcon className="size-4" />,
    accentColor: '#6366F1',
    accentSoftColor: 'rgba(99, 102, 241, 0.18)',
  },
  {
    type: 'tool',
    label: 'Tools',
    description: 'Invoke integrations or utility helpers.',
    icon: <ToolsIcon className="size-4" />,
    accentColor: '#F97316',
    accentSoftColor: 'rgba(249, 115, 22, 0.18)',
  },
  {
    type: 'guardrail',
    label: 'Guardrail',
    description: 'Add safety checks before continuing.',
    icon: <GuardrailIcon className="size-4" />,
    accentColor: '#F87171',
    accentSoftColor: 'rgba(248, 113, 113, 0.2)',
  },
  {
    type: 'note',
    label: 'Note',
    description: 'Leave documentation or context for teammates.',
    icon: <NoteIcon className="size-4" />,
    accentColor: '#FACC15',
    accentSoftColor: 'rgba(250, 204, 21, 0.22)',
  },
  {
    type: 'conditional',
    label: 'Condition',
    description: 'Branch the workflow based on outputs.',
    icon: <GitBranch className="size-4" />,
    accentColor: '#34D399',
    accentSoftColor: 'rgba(52, 211, 153, 0.2)',
  },
];

export const NODE_ACCENTS: Record<NodePaletteType, NodeAccent> =
  PALETTE_ITEMS.reduce<Record<NodePaletteType, NodeAccent>>(
    (acc, item) => {
      acc[item.type] = {
        icon: item.icon,
        accentColor: item.accentColor,
        accentSoftColor: item.accentSoftColor,
      };
      return acc;
    },
    {} as Record<NodePaletteType, NodeAccent>,
  );

export const INITIAL_WORKFLOW_PAYLOAD: WorkflowPayload = {
  edges: {
    edge_6lq6g9jp: {
      source: 'node_yzd2wh9k',
      target: 'node_k0ixmmrv',
      forward_condition: {
        label: null,
        type: 'unconditional',
      },
      backward_condition: null,
    },
    'xy-edge__node_k0ixmmrvnode_k0ixmmrv-on_result-node_0ks67g9dnode_0ks67g9d-target':
      {
        source: 'node_k0ixmmrv',
        target: 'node_0ks67g9d',
        forward_condition: {
          label: null,
          type: 'unconditional',
        },
        backward_condition: null,
      },
    'xy-edge__node_0ks67g9dnode_0ks67g9d-case-0-node_tw9czouunode_tw9czouu-target':
      {
        source: 'node_0ks67g9d',
        target: 'node_tw9czouu',
        forward_condition: {
          label: 'case-0',
          type: 'expression',
          expression: {
            type: 'cel',
            source: 'input.output_parsed.classification == "compare"',
          },
        },
        backward_condition: null,
      },
    edge_node_0ks67g9d_case_1: {
      source: 'node_0ks67g9d',
      target: 'node_7z5sdp3w',
      forward_condition: {
        label: 'case-1',
        type: 'expression',
        expression: {
          type: 'cel',
          source: 'input.output_parsed.classification == "answer_question"',
        },
      },
      backward_condition: null,
    },
    'xy-edge__node_0ks67g9dnode_0ks67g9d-fallback-node_0pt6jf9onode_0pt6jf9o-target':
      {
        source: 'node_0ks67g9d',
        target: 'node_0pt6jf9o',
        forward_condition: {
          label: 'fallback',
          type: 'expression',
          expression: {
            type: 'cel',
            source: 'true',
          },
        },
        backward_condition: null,
      },
  },
  nodes: {
    node_yzd2wh9k: {
      type: 'start',
      position: { x: -460, y: -80 },
      label: 'Start',
      edge_order: ['edge_6lq6g9jp'],
    },
    node_k0ixmmrv: {
      type: 'agent',
      position: { x: -200, y: -80 },
      label: 'Triage',
      llm: 'ChatGPT 5',
      instructions: `You are an assistant that gathers the key details needed to create a business initiative plan.

Look through the conversation to extract the following:
1. Initiative goal (what the team or organization aims to achieve)
2. Target completion date or timeframe
3. Available resources or current capacity (e.g., headcount, budget, or tool access)

If all three details are present anywhere in the conversation, return:
{
  "has_all_details": true,
  "initiative_goal": "<user-provided goal>",
  "target_timeframe": "<user-provided date or period>",
  "current_resources": "<user-provided resources>"
}`,
    },
    node_0ks67g9d: {
      type: 'conditional',
      position: { x: 60, y: -80 },
      label: 'If / else',
      config: {
        type: 'expression',
        expression: 'input.output_parsed.classification',
        llmDraft: {
          model: 'gpt-4.1-mini',
          prompt: 'Do this if: ',
        },
      },
    },
    node_tw9czouu: {
      type: 'agent',
      position: { x: 320, y: -180 },
      label: 'Launch helper',
      llm: 'ChatGPT 5',
      instructions: `Come up with a tailored plan to help the user run a new business initiative. Consider all the details they've provided and offer a succinct, bullet point list for how to run the initiative.

Use the web search tool to get additional context and synthesize a succinct answer that clearly explains how to run the project, identifying unique opportunities, highlighting risks and laying out mitigations that make sense.`,
    },
    node_7z5sdp3w: {
      type: 'agent',
      position: { x: 320, y: -40 },
      label: 'Answer helper',
      llm: 'ChatGPT 5',
      instructions: `Provide a concise, helpful answer to the user's question using the available context. Prefer actionable guidance and call out any missing information you need to respond fully.`,
    },
    node_0pt6jf9o: {
      type: 'agent',
      position: { x: 320, y: 20 },
      label: 'Get data',
      llm: 'ChatGPT 5',
      instructions: `Collect the missing data from the user.

Look through the conversation to extract the following:
1. Initiative goal (what the team or organization aims to achieve)
2. Target completion date or timeframe
3. Available resources or current capacity (e.g., headcount, budget, or tool access)

Make sure they are provided, be concise.`,
    },
  },
};
