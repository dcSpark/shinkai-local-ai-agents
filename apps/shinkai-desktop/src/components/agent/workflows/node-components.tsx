import { Textarea } from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { PlayIcon } from 'lucide-react';
import { Fragment, useCallback } from 'react';
import { type NodeProps, Handle, Position, useReactFlow } from 'reactflow';

import { DEFAULT_LLM, NODE_ACCENTS, NODE_TYPE_LABELS } from './constants';
import { type NodeAccent, type WorkflowNodeData } from './types';

const SharedHandles = ({
  showTarget = true,
  showSource = true,
  accentColor = '#6366F1',
}: {
  showTarget?: boolean;
  showSource?: boolean;
  accentColor?: string;
}) => (
  <Fragment>
    {showTarget && (
      <Handle
        className="!border-transparent"
        position={Position.Left}
        style={{ insetInlineStart: '-12px', backgroundColor: accentColor }}
        type="target"
      />
    )}
    {showSource && (
      <Handle
        className="!border-transparent"
        position={Position.Right}
        style={{ insetInlineEnd: '-12px', backgroundColor: accentColor }}
        type="source"
      />
    )}
  </Fragment>
);

const AgentWorkflowNode = ({ data, selected }: NodeProps<WorkflowNodeData>) => {
  const accent = NODE_ACCENTS.agent;
  const { accentColor, accentSoftColor, icon } = accent;
  return (
    <div
      className={cn(
        'relative min-w-[190px] rounded-full border p-2 shadow-sm transition-all duration-200',
        'flex items-center gap-3 backdrop-blur-md',
      )}
      style={{
        borderColor: selected ? accentColor : accentSoftColor,
        boxShadow: selected
          ? `0 16px 32px -18px ${accentSoftColor}`
          : undefined,
      }}
    >
      <div
        className="flex h-9 w-9 items-center justify-center rounded-full"
        style={{
          backgroundColor: accentSoftColor,
          color: accentColor,
        }}
      >
        {icon}
      </div>
      <div className="flex flex-col">
        <span className="text-sm leading-4 font-semibold">
          {data.agentConfig?.name ?? data.label}
        </span>
        <span className="text-text-tertiary text-xs">
          {data.agentConfig?.llm ?? DEFAULT_LLM} · Agent
        </span>
      </div>
      <SharedHandles accentColor={accentColor} />
    </div>
  );
};

const StartWorkflowNode = ({ data, selected }: NodeProps<WorkflowNodeData>) => (
  <div
    className={cn(
      'relative min-w-[150px] rounded-full border p-2 transition-all duration-200',
      'flex items-center gap-2 border-emerald-500/40 shadow-sm',
      selected && 'border-emerald-400/80 ring-2 ring-emerald-400/30',
    )}
  >
    <div className="flex h-8 w-8 items-center justify-center rounded-full bg-emerald-500/20 text-emerald-400">
      <PlayIcon className="size-4" />
    </div>
    <span className="text-xs font-medium text-emerald-100">
      {data.label ?? 'Start'}
    </span>
    <SharedHandles accentColor="#34D399" showTarget={false} />
  </div>
);

const GenericWorkflowNode = ({
  data,
  selected,
  accent,
  disableHandles = false,
}: NodeProps<WorkflowNodeData> & {
  accent: NodeAccent;
  disableHandles?: boolean;
}) => {
  const { accentColor, accentSoftColor, icon } = accent;
  return (
    <div
      className={cn(
        'relative min-w-[170px] rounded-full border p-2 shadow-sm transition-all duration-200',
        'flex items-center gap-3 backdrop-blur-md',
      )}
      style={{
        borderColor: selected ? accentColor : accentSoftColor,
        boxShadow: selected
          ? `0 14px 28px -18px ${accentSoftColor}`
          : undefined,
      }}
    >
      <div
        className="flex h-8 w-8 items-center justify-center rounded-full"
        style={{
          backgroundColor: accentSoftColor,
          color: accentColor,
        }}
      >
        {icon}
      </div>
      <div className="flex flex-col">
        <span className="text-sm leading-4 font-medium">{data.label}</span>
        <span className="text-text-tertiary text-xs">
          {NODE_TYPE_LABELS[data.nodeType]}
        </span>
      </div>
      {!disableHandles && <SharedHandles accentColor={accentColor} />}
    </div>
  );
};

const StickyNoteNode = ({
  id,
  data,
  selected,
  accent,
}: NodeProps<WorkflowNodeData> & { accent: NodeAccent }) => {
  const reactFlow = useReactFlow<WorkflowNodeData>();

  const handleContentChange = useCallback(
    (value: string) => {
      reactFlow.setNodes((current) =>
        current.map((node) =>
          node.id === id
            ? {
                ...node,
                data: {
                  ...node.data,
                  description: value,
                },
              }
            : node,
        ),
      );
    },
    [id, reactFlow],
  );

  return (
    <div
      className={cn(
        'relative max-w-[260px] min-w-[220px] rounded-2xl border p-4 shadow-sm transition-transform duration-200',
        'text-text-default bg-yellow-900/20 backdrop-blur-sm',
      )}
      style={{
        borderColor: selected ? accent.accentColor : accent.accentSoftColor,
      }}
      onPointerDown={(event) => event.stopPropagation()}
    >
      <Textarea
        className="placeholder:text-text-placeholder text-text-default min-h-[120px] w-full resize-none border-none bg-transparent p-0 text-sm focus-visible:ring-0"
        onChange={(event) => handleContentChange(event.target.value)}
        placeholder="Write down context or reminders for your workflow."
        value={data.description ?? ''}
      />
    </div>
  );
};

export const nodeTypes = {
  agentNode: AgentWorkflowNode,
  startNode: StartWorkflowNode,
  toolNode: (props: NodeProps<WorkflowNodeData>) => (
    <GenericWorkflowNode {...props} accent={NODE_ACCENTS.tool} />
  ),
  guardrailNode: (props: NodeProps<WorkflowNodeData>) => (
    <GenericWorkflowNode {...props} accent={NODE_ACCENTS.guardrail} />
  ),
  conditionalNode: (props: NodeProps<WorkflowNodeData>) => (
    <GenericWorkflowNode {...props} accent={NODE_ACCENTS.conditional} />
  ),
  noteNode: (props: NodeProps<WorkflowNodeData>) => (
    <StickyNoteNode {...props} accent={NODE_ACCENTS.note} />
  ),
};

export {
  SharedHandles,
  AgentWorkflowNode,
  StartWorkflowNode,
  GenericWorkflowNode,
  StickyNoteNode,
};
