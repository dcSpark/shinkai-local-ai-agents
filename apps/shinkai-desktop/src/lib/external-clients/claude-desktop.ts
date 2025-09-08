import { invoke } from '@tauri-apps/api/core';
import { type TFunction } from 'i18next'; // Import TFunction from i18next core
import { toast } from 'sonner';

import { useAuth } from '../../store/auth'; // Adjust path as needed
import { ConfigError, getDenoBinPath } from './common'; // Import from common.ts

export interface McpServerConfig {
  command: string;
  args: string[];
}

export const getClaudeDesktopConfigPath = (): string => {
  const platform = window.navigator.platform.toLowerCase();

  if (platform.includes('win')) {
    return '%APPDATA%\\Claude\\claude_desktop_config.json';
  } else if (platform.includes('mac')) {
    return '~/Library/Application Support/Claude/claude_desktop_config.json';
  } else {
    return '~/.config/Claude/claude_desktop_config.json';
  }
};

export const handleConfigureClaude = async (serverId: string, t: TFunction) => {
  const auth = useAuth.getState().auth;
  const nodeUrl = auth?.node_address || 'http://localhost:9550';
  const loadingToastId = toast.loading(t('mcpClients.claudeLoading'));

  try {
    const denoBinPath = await getDenoBinPath();
    const command = denoBinPath;
    const args = [
      'run',
      '-A',
      'npm:supergateway@3.4.0',
      '--sse',
      `${nodeUrl}/mcp/sse`,
    ];

    // Call the actual invoke function
    await invoke('register_server_in_claude', {
      serverId: serverId,
      binaryPath: command,
      serverArgs: args,
    });
    toast.success(t('mcpClients.claudeSuccessTitle'), {
      id: loadingToastId,
      description: t('mcpClients.claudeSuccessDescription'),
    });
  } catch (error) {
    toast.dismiss(loadingToastId);
    console.error('Automatic Claude Desktop configuration failed:', error);
    let errorMessage = t('mcpClients.claudeFailMessageBase');
    if (typeof error === 'string') {
      errorMessage += ` Error: ${error}`;
    } else if (error instanceof Error) {
      errorMessage += ` Error: ${error.message}`;
    }

    let helpText: string | null = null; // Declare helpText here
    try {
      const command = 'npx';
      const args = ['-y', 'supergateway@3.4.0', '--sse', `${nodeUrl}/mcp/sse`];
      helpText = await invoke<string>('get_claude_config_help', {
        serverId: serverId,
        binaryPath: command,
        serverArgs: args,
      });
    } catch (helpError) {
      console.error('Failed to fetch Claude config help text:', helpError);
      throw new Error(
        `${errorMessage} Could not retrieve manual setup instructions.`,
      );
    }

    if (helpText) {
      throw new ConfigError(errorMessage, helpText);
    } else {
      throw new Error(
        `${errorMessage} Could not retrieve manual setup instructions (unexpected state).`,
      );
    }
  }
};
