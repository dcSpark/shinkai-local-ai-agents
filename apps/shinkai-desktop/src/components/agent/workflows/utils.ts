import { type XYPosition } from 'reactflow';
import { v4 as uuid } from 'uuid';

import {
  DEFAULT_LLM,
  INITIAL_WORKFLOW_PAYLOAD,
  MAX_SPACING_ITERATIONS,
  MIN_NODE_DISTANCE,
  NODE_TYPE_LABELS,
} from './constants';
import {
  type ConditionalConfig,
  type NodePaletteType,
  type WorkflowNode,
  type WorkflowEdge,
  type WorkflowPayload,
  type WorkflowState,
  type WorkflowNodeType,
} from './types';

export const enforceSpacing = (
  inputNodes: WorkflowNode[],
  resolver: (position: XYPosition, existingNodes: WorkflowNode[]) => XYPosition,
  anchorId?: string,
): WorkflowNode[] => {
  const mutable = inputNodes.map((node) => ({
    ...node,
    position: { ...node.position },
  }));

  for (let iteration = 0; iteration < MAX_SPACING_ITERATIONS; iteration++) {
    let adjustedOnPass = false;
    for (let i = 0; i < mutable.length; i++) {
      for (let j = i + 1; j < mutable.length; j++) {
        const nodeA = mutable[i];
        const nodeB = mutable[j];
        let dx = nodeB.position.x - nodeA.position.x;
        let dy = nodeB.position.y - nodeA.position.y;
        let distance = Math.sqrt(dx * dx + dy * dy);

        if (distance === 0) {
          dx = 1;
          dy = 0;
          distance = 0.001;
        }

        if (distance < MIN_NODE_DISTANCE) {
          const overlap = MIN_NODE_DISTANCE - distance;
          const ux = dx / distance;
          const uy = dy / distance;

          if (nodeA.id === anchorId && nodeB.id !== anchorId) {
            nodeB.position.x += ux * overlap;
            nodeB.position.y += uy * overlap;
          } else if (nodeB.id === anchorId && nodeA.id !== anchorId) {
            nodeA.position.x -= ux * overlap;
            nodeA.position.y -= uy * overlap;
          } else {
            nodeA.position.x -= ux * (overlap / 2);
            nodeA.position.y -= uy * (overlap / 2);
            nodeB.position.x += ux * (overlap / 2);
            nodeB.position.y += uy * (overlap / 2);
          }

          adjustedOnPass = true;
        }
      }
    }

    if (!adjustedOnPass) {
      break;
    }
  }

  const ordered = anchorId
    ? mutable.sort((a, b) =>
        a.id === anchorId ? -1 : b.id === anchorId ? 1 : 0,
      )
    : mutable;

  const finalized: WorkflowNode[] = [];
  ordered.forEach((node) => {
    if (node.id === anchorId) {
      finalized.push(node);
    } else {
      const adjusted = resolver(node.position, finalized);
      finalized.push({ ...node, position: adjusted });
    }
  });

  return finalized;
};

const getFlowType = (nodeType: WorkflowNodeType) => {
  switch (nodeType) {
    case 'agent':
      return 'agentNode';
    case 'tool':
      return 'toolNode';
    case 'conditional':
      return 'conditionalNode';
    case 'note':
      return 'noteNode';
    case 'guardrail':
      return 'guardrailNode';
    case 'start':
    default:
      return 'startNode';
  }
};

const createNodeData = (
  id: string,
  node: WorkflowPayload['nodes'][string],
): WorkflowNode => {
  const nodeType = (node.type ?? 'tool') as WorkflowNodeType;
  const label = node.label ?? NODE_TYPE_LABELS[nodeType];
  return {
    id,
    type: getFlowType(nodeType),
    position: node.position,
    data: {
      label,
      nodeType,
      description:
        nodeType === 'note'
          ? (node.content ?? node.description ?? '')
          : nodeType === 'conditional'
            ? 'Configure the condition to branch.'
            : undefined,
      agentConfig:
        nodeType === 'agent'
          ? {
              name: label,
              llm: node.llm ?? DEFAULT_LLM,
              instructions: (node.instructions as string) ?? '',
            }
          : undefined,
      conditionalConfig:
        nodeType === 'conditional'
          ? (node.config as ConditionalConfig | undefined)
          : undefined,
    },
  };
};

export const transformInitialData = (): WorkflowState => {
  const nodes: WorkflowNode[] = Object.entries(
    INITIAL_WORKFLOW_PAYLOAD.nodes,
  ).map(([id, node]) => createNodeData(id, node));

  const edges: WorkflowEdge[] = Object.entries(
    INITIAL_WORKFLOW_PAYLOAD.edges,
  ).map(([id, edge]) => ({
    id,
    source: edge.source,
    target: edge.target,
    label:
      edge.forward_condition?.type === 'unconditional'
        ? undefined
        : (edge.forward_condition?.label ?? undefined),
    data: edge.forward_condition ?? undefined,
    animated: edge.forward_condition?.type === 'conditional',
  }));

  return { nodes, edges };
};

export const generateNode = (
  type: NodePaletteType,
  position?: XYPosition,
): WorkflowNode => ({
  id: uuid(),
  type: getFlowType(type),
  position: {
    x: position?.x ?? Math.random() * 240,
    y: position?.y ?? Math.random() * 240,
  },
  data: {
    label: `${NODE_TYPE_LABELS[type]} Node`,
    description:
      type === 'note'
        ? ''
        : type === 'conditional'
          ? 'Configure the condition to branch.'
          : undefined,
    nodeType: type,
    agentConfig:
      type === 'agent'
        ? {
            name: 'New Agent',
            llm: DEFAULT_LLM,
            instructions: '',
          }
        : undefined,
    conditionalConfig:
      type === 'conditional'
        ? {
            type: 'expression',
            expression: 'true',
            llmDraft: {
              model: 'gpt-4.1-mini',
              prompt: 'Do this if: ',
            },
          }
        : undefined,
  },
});
