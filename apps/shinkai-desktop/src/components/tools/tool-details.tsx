import {
  type AgentShinkaiTool,
  type DenoShinkaiTool,
  type NetworkShinkaiTool,
  type PythonShinkaiTool,
  type RustShinkaiTool,
  type McpServerTool as McpServerToolType,
  type ShinkaiTool,
} from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { useGetPlaygroundTools } from '@shinkai_network/shinkai-node-state/v2/queries/getPlaygroundTools/useGetPlaygroundTools';
import { useGetTool } from '@shinkai_network/shinkai-node-state/v2/queries/getTool/useGetTool';
import { Skeleton } from '@shinkai_network/shinkai-ui';
import { MoreVertical } from 'lucide-react';
import { useParams } from 'react-router';

import { SubpageLayout } from '../../pages/layout/simple-layout';
import { useAuth } from '../../store/auth';
import AgentTool from './agent-tool';
import DenoTool from './deno-tool';
import McpServerTool from './mcp-server-tool';
import NetworkTool from './network-tool';
import PythonTool from './python-tool';
import RustTool from './rust-tool';

export function isDenoShinkaiTool(tool: ShinkaiTool): tool is DenoShinkaiTool {
  return (tool as DenoShinkaiTool).js_code !== undefined;
}

export function isPythonShinkaiTool(
  tool: ShinkaiTool,
): tool is PythonShinkaiTool {
  return (tool as PythonShinkaiTool).py_code !== undefined;
}

export default function ToolDetails() {
  const auth = useAuth((state) => state.auth);

  const { toolKey } = useParams();
  const { data, isSuccess, isPending } = useGetTool({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    toolKey: toolKey ?? '',
  });

  const { data: playgroundTools } = useGetPlaygroundTools({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const tool = data?.content[0] as ShinkaiTool;
  const isEnabled = data?.content[1] as boolean;

  const toolType = data?.type;

  if (isPending) {
    return (
      <SubpageLayout alignLeft className="container" title="">
        <div className="mb-8 flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Skeleton className="h-16 w-16 rounded-md" />
            <div className="flex flex-col gap-2">
              <Skeleton className="h-7 w-64 rounded-md" />
              <Skeleton className="h-5 w-96 rounded-md" />
            </div>
          </div>
          <div className="flex items-center gap-4">
            <Skeleton className="h-6 w-24 rounded-md" />{' '}
            <div className="flex h-8 w-8 items-center justify-center">
              <MoreVertical className="h-5 w-5 text-gray-400" />
            </div>
          </div>
        </div>

        <div className="mb-8 border-b border-gray-500 pb-2">
          <div className="flex gap-2">
            <Skeleton className="h-6 w-20 rounded-md" />
            <Skeleton className="h-6 w-20 rounded-md" />
            <Skeleton className="h-6 w-32 rounded-md" />
          </div>
        </div>
        <div className="flex flex-col gap-6 px-4">
          <div className="flex flex-col gap-2">
            <Skeleton className="h-5 w-32 rounded-md" />
            <Skeleton className="h-6 w-full max-w-2xl rounded-md" />
          </div>

          <div className="mt-4 flex flex-col gap-2">
            <Skeleton className="h-5 w-24 rounded-md" />
            <Skeleton className="h-6 w-24 rounded-md" />
          </div>

          <div className="mt-4 flex flex-col gap-2">
            <Skeleton className="h-5 w-20 rounded-md" />
            <Skeleton className="h-6 w-48 rounded-md" />
          </div>

          <div className="mt-4 flex flex-col gap-2">
            <Skeleton className="h-5 w-24 rounded-md" />
            <Skeleton className="h-6 w-full max-w-xl rounded-md" />
          </div>
        </div>
      </SubpageLayout>
    );
  }

  if (isSuccess && toolType === 'Deno') {
    return (
      <SubpageLayout className="container" title="">
        <DenoTool
          isEnabled={isEnabled}
          isPlaygroundTool={playgroundTools?.some(
            (playgroundTool) => playgroundTool.tool_router_key === toolKey,
          )}
          tool={tool as DenoShinkaiTool}
          toolRouterKey={toolKey ?? ''}
        />
      </SubpageLayout>
    );
  } else if (isSuccess && toolType === 'Python') {
    return (
      <SubpageLayout className="container" title="">
        <PythonTool
          isEnabled={isEnabled}
          isPlaygroundTool={playgroundTools?.some(
            (playgroundTool) => playgroundTool.tool_router_key === toolKey,
          )}
          tool={tool as PythonShinkaiTool}
          toolRouterKey={toolKey ?? ''}
        />
      </SubpageLayout>
    );
  } else if (isSuccess && toolType === 'Rust') {
    return (
      <SubpageLayout className="container" title="">
        <RustTool
          isEnabled={isEnabled}
          tool={tool as RustShinkaiTool}
          toolRouterKey={toolKey ?? ''}
        />
      </SubpageLayout>
    );
  } else if (isSuccess && toolType === 'Network') {
    return (
      <SubpageLayout className="container" title="">
        <NetworkTool
          isEnabled={isEnabled}
          tool={tool as NetworkShinkaiTool}
          toolRouterKey={toolKey ?? ''}
        />
      </SubpageLayout>
    );
  } else if (isSuccess && toolType === 'Agent') {
    return (
      <SubpageLayout className="container" title="">
        <AgentTool
          isEnabled={isEnabled}
          isPlaygroundTool={playgroundTools?.some(
            (playgroundTool) => playgroundTool.tool_router_key === toolKey,
          )}
          tool={tool as AgentShinkaiTool}
          toolRouterKey={toolKey ?? ''}
        />
      </SubpageLayout>
    );
  } else if (isSuccess && toolType === 'MCPServer') {
    return (
      <SubpageLayout className="container" title="">
        <McpServerTool
          isEnabled={isEnabled}
          isPlaygroundTool={playgroundTools?.some(
            (playgroundTool) => playgroundTool.tool_router_key === toolKey,
          )}
          tool={tool as McpServerToolType}
          toolRouterKey={toolKey ?? ''}
        />
      </SubpageLayout>
    );
  } else {
    return <div>Tool not found</div>;
  }
}
