import ReactFlow, {
  Background,
  Controls,
  ReactFlowProvider,
  addEdge,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Connection,
  type Edge,
  type XYPosition,
} from 'reactflow';
import 'reactflow/dist/style.css';
// eslint-disable-next-line import/order
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
} from 'react';
import { v4 as uuid } from 'uuid';

import { DEFAULT_LLM, MIN_NODE_DISTANCE } from './workflows/constants';
import { nodeTypes } from './workflows/node-components';
import WorkflowNodeDetails from './workflows/node-details';
import {
  type NodePaletteType,
  type WorkflowNode,
  type WorkflowNodeData,
} from './workflows/types';
import {
  enforceSpacing,
  generateNode,
  transformInitialData,
} from './workflows/utils';
import WorkflowPalette from './workflows/workflow-palette';

type WorkflowEditorProps = {
  onDirtyChange?: (dirty: boolean) => void;
  baselineKey?: number;
  restoreKey?: number;
};

const INITIAL_WORKFLOW = transformInitialData();

const cloneWorkflowNode = (node: WorkflowNode): WorkflowNode => ({
  ...node,
  position: { ...node.position },
  data: {
    ...node.data,
    agentConfig: node.data.agentConfig
      ? { ...node.data.agentConfig }
      : undefined,
    conditionalConfig: node.data.conditionalConfig
      ? (JSON.parse(
          JSON.stringify(node.data.conditionalConfig),
        ) as WorkflowNodeData['conditionalConfig'])
      : undefined,
  },
});

const cloneWorkflowEdge = (edge: Edge): Edge => ({
  ...edge,
  data:
    edge.data && typeof edge.data === 'object'
      ? { ...(edge.data as Record<string, unknown>) }
      : edge.data,
});

const serializeWorkflowState = (
  nodes: WorkflowNode[],
  edges: Edge[],
): string => {
  const safeNodes = nodes
    .map((node) => ({
      id: node.id,
      type: node.type,
      position: node.position,
      data: node.data,
    }))
    .sort((a, b) => a.id.localeCompare(b.id));

  const safeEdges = edges
    .map((edge) => ({
      id: edge.id,
      source: edge.source,
      target: edge.target,
      sourceHandle: edge.sourceHandle,
      targetHandle: edge.targetHandle,
      data: edge.data,
    }))
    .sort((a, b) => a.id.localeCompare(b.id));

  return JSON.stringify({ nodes: safeNodes, edges: safeEdges });
};

const WorkflowCanvas = ({
  onDirtyChange,
  baselineKey = 0,
  restoreKey = 0,
}: WorkflowEditorProps) => {
  const [nodes, setNodes, onNodesChange] = useNodesState(
    INITIAL_WORKFLOW.nodes,
  );
  const [edges, setEdges, onEdgesChange] = useEdgesState(
    INITIAL_WORKFLOW.edges,
  );
  const reactFlow = useReactFlow<WorkflowNodeData>();
  const reactFlowWrapper = useRef<HTMLDivElement | null>(null);
  const nodesRef = useRef<WorkflowNode[]>(nodes);
  const edgesRef = useRef<Edge[]>(edges);
  useEffect(() => {
    nodesRef.current = nodes;
  }, [nodes]);
  useEffect(() => {
    edgesRef.current = edges;
  }, [edges]);

  const baselineNodesRef = useRef<WorkflowNode[]>(
    INITIAL_WORKFLOW.nodes.map(cloneWorkflowNode),
  );
  const baselineEdgesRef = useRef<Edge[]>(
    INITIAL_WORKFLOW.edges.map(cloneWorkflowEdge),
  );
  const baselineSerializedRef = useRef(
    serializeWorkflowState(baselineNodesRef.current, baselineEdgesRef.current),
  );
  const lastDirtyRef = useRef(false);

  useEffect(() => {
    const isDirty =
      serializeWorkflowState(nodes, edges) !== baselineSerializedRef.current;
    if (lastDirtyRef.current !== isDirty) {
      lastDirtyRef.current = isDirty;
      onDirtyChange?.(isDirty);
    }
  }, [nodes, edges, onDirtyChange]);

  useEffect(() => {
    baselineNodesRef.current = nodesRef.current.map(cloneWorkflowNode);
    baselineEdgesRef.current = edgesRef.current.map(cloneWorkflowEdge);
    baselineSerializedRef.current = serializeWorkflowState(
      baselineNodesRef.current,
      baselineEdgesRef.current,
    );
    lastDirtyRef.current = false;
    onDirtyChange?.(false);
  }, [baselineKey, onDirtyChange]);

  const restoreInitRef = useRef(true);
  useEffect(() => {
    if (restoreInitRef.current) {
      restoreInitRef.current = false;
      return;
    }

    const nextNodes = baselineNodesRef.current.map(cloneWorkflowNode);
    const nextEdges = baselineEdgesRef.current.map(cloneWorkflowEdge);
    setNodes(nextNodes);
    setEdges(nextEdges);
    nodesRef.current = nextNodes;
    edgesRef.current = nextEdges;
    lastDirtyRef.current = false;
    onDirtyChange?.(false);
  }, [restoreKey, setEdges, setNodes, onDirtyChange]);

  const [selectedType, setSelectedType] = useState<NodePaletteType>('agent');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  const onConnect = useCallback(
    (connection: Connection) => {
      setEdges((current) =>
        addEdge(
          {
            ...connection,
            id: uuid(),
            style: { strokeWidth: 2 },
          },
          current,
        ),
      );
    },
    [setEdges],
  );

  const resolvePosition = useCallback(
    (desired: XYPosition, existingNodes: WorkflowNode[]): XYPosition => {
      const STEP = 48;
      const MAX_ATTEMPTS = 120;

      if (existingNodes.length === 0) {
        return desired;
      }

      let attempt = 0;
      let radius = 0;
      let angle = 0;
      let candidate = { ...desired };

      const isOverlapping = ({ x, y }: XYPosition) =>
        existingNodes.some((node) => {
          const dx = node.position.x - x;
          const dy = node.position.y - y;
          return Math.sqrt(dx * dx + dy * dy) < MIN_NODE_DISTANCE;
        });

      while (attempt < MAX_ATTEMPTS && isOverlapping(candidate)) {
        angle += Math.PI / 4;
        radius += STEP;
        candidate = {
          x: desired.x + radius * Math.cos(angle),
          y: desired.y + radius * Math.sin(angle),
        };
        attempt += 1;
      }

      return candidate;
    },
    [],
  );

  const createNodeAt = useCallback(
    async (type: NodePaletteType, position?: XYPosition) => {
      const basePosition = position ?? { x: 0, y: 0 };
      const adjusted = resolvePosition(basePosition, nodesRef.current);
      const nextNode = generateNode(type, adjusted);
      const spaced = enforceSpacing(
        [...nodesRef.current, nextNode],
        (pos, existing) => resolvePosition(pos, existing),
        nextNode.id,
      );
      nodesRef.current = spaced;
      setNodes(spaced);
      setSelectedNodeId(nextNode.id);
    },
    [resolvePosition, setNodes],
  );

  const focusPositionForPalette = (): XYPosition | undefined => {
    if (!reactFlowWrapper.current) return undefined;
    const rect = reactFlowWrapper.current.getBoundingClientRect();
    return reactFlow.project({
      x: rect.width / 2,
      y: rect.height / 2,
    });
  };

  const handlePaletteSelect = (type: NodePaletteType) => {
    setSelectedType(type);
    const center = focusPositionForPalette() ?? { x: 0, y: 0 };
    const newNode = generateNode(type, center);
    let updatedNodes: WorkflowNode[] = [];
    setNodes((current) => {
      updatedNodes = [...current, newNode];
      return updatedNodes;
    });
    nodesRef.current = updatedNodes;
    setSelectedNodeId(newNode.id);
  };

  const handleDragStart = (
    event: DragEvent<HTMLButtonElement>,
    type: NodePaletteType,
  ) => {
    setSelectedType(type);
    event.dataTransfer.setData('application/reactflow/node-type', type);
    event.dataTransfer.effectAllowed = 'move';
  };

  const handleDragOver = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  };

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    const type = event.dataTransfer.getData(
      'application/reactflow/node-type',
    ) as NodePaletteType;
    if (!type) return;
    setSelectedType(type);
    const bounds = reactFlowWrapper.current?.getBoundingClientRect();
    const position = bounds
      ? reactFlow.project({
          x: event.clientX - bounds.left,
          y: event.clientY - bounds.top,
        })
      : reactFlow.project({ x: event.clientX, y: event.clientY });
    void createNodeAt(type, position);
  };

  useEffect(() => {
    if (selectedNodeId && !nodes.some((node) => node.id === selectedNodeId)) {
      setSelectedNodeId(null);
    }
  }, [nodes, selectedNodeId]);

  const selectedNode = useMemo(
    () =>
      selectedNodeId
        ? (nodes.find((node) => node.id === selectedNodeId) ?? null)
        : null,
    [nodes, selectedNodeId],
  );

  const handleAgentChange = useCallback(
    (
      updater: (
        current: Required<WorkflowNodeData['agentConfig']>,
      ) => Required<WorkflowNodeData['agentConfig']>,
    ) => {
      if (!selectedNodeId) return;
      setNodes((current) =>
        current.map((node) => {
          if (node.id !== selectedNodeId || node.data?.nodeType !== 'agent') {
            return node;
          }
          const currentConfig: Required<WorkflowNodeData['agentConfig']> = {
            name: node.data.agentConfig?.name ?? node.data.label,
            llm: node.data.agentConfig?.llm ?? DEFAULT_LLM,
            instructions: node.data.agentConfig?.instructions ?? '',
          };
          const nextConfig = updater(currentConfig);
          return {
            ...node,
            data: {
              ...node.data,
              label: nextConfig.name,
              agentConfig: nextConfig,
            },
          };
        }),
      );
    },
    [selectedNodeId, setNodes],
  );

  const handleNodeLabelChange = useCallback(
    (label: string) => {
      if (!selectedNodeId) return;
      setNodes((current) =>
        current.map((node) =>
          node.id === selectedNodeId
            ? {
                ...node,
                data: {
                  ...node.data,
                  label,
                },
              }
            : node,
        ),
      );
    },
    [selectedNodeId, setNodes],
  );

  return (
    <div className="relative h-full w-full">
      <div className="h-full w-full" ref={reactFlowWrapper}>
        <ReactFlow
          className="h-full w-full"
          edges={edges}
          nodeTypes={nodeTypes}
          nodes={nodes}
          onConnect={onConnect}
          onDragOver={handleDragOver}
          onDrop={handleDrop}
          onEdgesChange={onEdgesChange}
          onNodeClick={(_, node) => setSelectedNodeId(node.id)}
          onNodesChange={onNodesChange}
          onPaneClick={() => setSelectedNodeId(null)}
          fitView
          maxZoom={1}
          minZoom={0.5}
        >
          <Background color="hsl(var(--muted-foreground))" gap={18} size={1} />
          <Controls
            className="border-divider bg-bg-default/90 rounded-full border shadow-lg [&>button]:rounded-full"
            position="bottom-left"
            showZoom
            showFitView
            showInteractive
          />
        </ReactFlow>
      </div>

      <div className="pointer-events-none absolute top-6 left-6 z-30 w-64 space-y-4">
        <WorkflowPalette
          onDragStart={handleDragStart}
          onSelect={handlePaletteSelect}
          selectedType={selectedType}
        />
      </div>

      {/* <div className="pointer-events-none absolute right-6 bottom-6 z-20 w-[min(420px,calc(100vw-48px))]">
        <Card className="border-divider bg-bg-default/90 pointer-events-auto border backdrop-blur">
          <CardHeader className="pb-2">
            <CardTitle className="text-text-secondary text-sm">
              Workflow JSON
            </CardTitle>
          </CardHeader>
          <CardContent>
            <pre className="bg-bg-secondary/80 text-text-tertiary max-h-[220px] overflow-auto rounded-xl p-3 text-[11px] leading-relaxed">
              {serializedWorkflow}
            </pre>
          </CardContent>
        </Card>
      </div> */}

      {selectedNode && (
        <div className="pointer-events-none absolute top-6 right-6 z-30 w-[320px]">
          <WorkflowNodeDetails
            node={selectedNode}
            onAgentChange={handleAgentChange}
            onLabelChange={handleNodeLabelChange}
          />
        </div>
      )}
    </div>
  );
};

const WorkflowEditor = ({
  onDirtyChange,
  baselineKey,
  restoreKey,
}: WorkflowEditorProps = {}) => (
  <ReactFlowProvider>
    <WorkflowCanvas
      baselineKey={baselineKey}
      onDirtyChange={onDirtyChange}
      restoreKey={restoreKey}
    />
  </ReactFlowProvider>
);

export default WorkflowEditor;
