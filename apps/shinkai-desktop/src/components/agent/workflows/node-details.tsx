import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Textarea,
} from '@shinkai_network/shinkai-ui';

import { DEFAULT_LLM, LLM_OPTIONS, NODE_TYPE_LABELS } from './constants';
import {
  type LLMOption,
  type WorkflowNode,
  type WorkflowNodeData,
} from './types';

type WorkflowNodeDetailsProps = {
  node: WorkflowNode | null;
  onAgentChange: (
    updater: (
      current: Required<WorkflowNodeData['agentConfig']>,
    ) => Required<WorkflowNodeData['agentConfig']>,
  ) => void;
  onLabelChange: (label: string) => void;
};

const AgentDetails = ({
  config,
  onChange,
}: {
  config: Required<WorkflowNodeData['agentConfig']>;
  onChange: (
    updater: (
      current: Required<WorkflowNodeData['agentConfig']>,
    ) => Required<WorkflowNodeData['agentConfig']>,
  ) => void;
}) => (
  <Card className="border-divider bg-bg-default/95 pointer-events-auto border shadow-xl">
    <CardHeader>
      <div className="flex items-start justify-between gap-2">
        <div>
          <CardTitle className="text-base">{config.name}</CardTitle>
          <p className="text-text-tertiary text-xs">
            Configure how this agent behaves in the workflow.
          </p>
        </div>
      </div>
    </CardHeader>
    <CardContent className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="agent-name">Name</Label>
        <Input
          id="agent-name"
          onChange={(event) =>
            onChange((current) => ({
              ...current,
              name: event.target.value,
            }))
          }
          placeholder="Enter agent name"
          value={config.name}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="agent-llm">Model</Label>
        <Select
          onValueChange={(value) =>
            onChange((current) => ({
              ...current,
              llm: value as LLMOption,
            }))
          }
          value={config.llm}
        >
          <SelectTrigger id="agent-llm">
            <SelectValue placeholder="Select model" />
          </SelectTrigger>
          <SelectContent>
            {LLM_OPTIONS.map((option) => (
              <SelectItem key={option} value={option}>
                {option}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="space-y-2">
        <Label htmlFor="agent-instructions">Instructions</Label>
        <Textarea
          id="agent-instructions"
          onChange={(event) =>
            onChange((current) => ({
              ...current,
              instructions: event.target.value,
            }))
          }
          placeholder="Describe what this agent should do"
          rows={4}
          value={config.instructions}
        />
      </div>
    </CardContent>
  </Card>
);

const GenericDetails = ({
  node,
  onLabelChange,
}: {
  node: WorkflowNode;
  onLabelChange: (label: string) => void;
}) => (
  <Card className="border-divider bg-bg-default/95 pointer-events-auto border shadow-xl">
    <CardHeader>
      <CardTitle className="text-base">
        {NODE_TYPE_LABELS[node.data.nodeType]}
      </CardTitle>
    </CardHeader>
    <CardContent className="space-y-3">
      <div className="space-y-2">
        <Label htmlFor="node-label">Display name</Label>
        <Input
          id="node-label"
          onChange={(event) => onLabelChange(event.target.value)}
          placeholder="Enter stage name"
          value={node.data.label}
        />
      </div>
      <p className="text-text-tertiary text-xs leading-relaxed">
        {node.data.nodeType === 'conditional'
          ? 'Conditions determine how the workflow branches. Update the display name so teammates understand what this check does.'
          : node.data.nodeType === 'note'
            ? 'Notes surface context for collaborators. Update the title to make it easier to spot.'
            : node.data.nodeType === 'tool'
              ? 'Tools trigger external integrations or utilities. Rename this step to clarify its purpose.'
              : node.data.nodeType === 'guardrail'
                ? 'Guardrails add safety checks before continuing. Give it a clear name that explains the protection it provides.'
                : 'Update this stage to keep your workflow easy to scan.'}
      </p>
    </CardContent>
  </Card>
);

const WorkflowNodeDetails = ({
  node,
  onAgentChange,
  onLabelChange,
}: WorkflowNodeDetailsProps) => {
  if (!node) return null;

  if (node.data.nodeType === 'agent') {
    const config: Required<WorkflowNodeData['agentConfig']> = {
      name: node.data.agentConfig?.name ?? node.data.label,
      llm: node.data.agentConfig?.llm ?? DEFAULT_LLM,
      instructions: node.data.agentConfig?.instructions ?? '',
    };
    return <AgentDetails config={config} onChange={onAgentChange} />;
  }

  if (node.data.nodeType === 'start') {
    return (
      <Card className="border-divider bg-bg-default/95 pointer-events-auto border shadow-xl">
        <CardHeader>
          <CardTitle className="text-base">Start node</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-text-tertiary text-sm leading-relaxed">
            The start node launches the workflow. Connect it to the first step
            to define how the automation begins.
          </p>
        </CardContent>
      </Card>
    );
  }

  return <GenericDetails node={node} onLabelChange={onLabelChange} />;
};

export default WorkflowNodeDetails;
