import { platform } from '@tauri-apps/plugin-os';

import { ModelProvider } from '../../components/ais/constants';
import OLLAMA_MODELS_REPOSITORY from './ollama-models-repository.json';

export enum OllamaModelQuality {
  Low = 'low',
  Medium = 'medium',
  Good = 'good',
}

export enum OllamaModelSpeed {
  Average = 'average',
  Fast = 'fast',
  VeryFast = 'very-fast',
}

export enum OllamaModelCapability {
  TextGeneration = 'text-generation',
  ImageToText = 'image-to-text',
  Thinking = 'thinking',
  ToolCalling = 'tool-calling',
}

export interface OllamaModel {
  model: string;
  tag: string;
  name: string;
  description: string;
  contextLength: number;
  quality: OllamaModelQuality;
  speed: OllamaModelSpeed;
  capabilities: OllamaModelCapability[];
  size: number; // Size in GB
  fullName: string;
  provider?: string;
}
export type OllamaModelDefinition =
  (typeof FILTERED_OLLAMA_MODELS_REPOSITORY)[0];

export const FILTERED_OLLAMA_MODELS_REPOSITORY =
  OLLAMA_MODELS_REPOSITORY.filter((model) => !model.embedding);
export const ALLOWED_OLLAMA_MODELS = FILTERED_OLLAMA_MODELS_REPOSITORY.flatMap(
  (model) => model.tags.map((tag) => tag.name),
);

const currentPlatform = platform();

export const OLLAMA_MODELS: OllamaModel[] = [
  ...(currentPlatform === 'windows' || currentPlatform === 'linux'
    ? [
        {
          model: 'gpt-oss',
          tag: '20b',
          name: 'gpt-oss',
          description:
            'OpenAI’s open-weight models designed for powerful reasoning, agentic tasks, and versatile developer use cases.',
          contextLength: 32000,
          quality: OllamaModelQuality.Good,
          speed: OllamaModelSpeed.Fast,
          capabilities: [
            OllamaModelCapability.TextGeneration,
            OllamaModelCapability.Thinking,
            OllamaModelCapability.ToolCalling,
          ],
          size: 7.5,
          fullName: '',
          provider: ModelProvider.OpenAI,
        },
        {
          model: 'llama3.1',
          tag: '8b-instruct-q4_1',
          name: 'Llama 3.1 8b',
          description:
            'A powerful AI model for understanding and generating text, optimized for tasks like writing and processing language',
          contextLength: 128000,
          quality: OllamaModelQuality.Medium,
          speed: OllamaModelSpeed.Fast,
          capabilities: [
          OllamaModelCapability.TextGeneration,
            OllamaModelCapability.ToolCalling,
          ],
          size: 4.7,
          fullName: '',
          provider: ModelProvider.Meta,
        },
      ]
    : []),
  ...(currentPlatform === 'macos'
    ? [
        {
          model: 'gpt-oss',
          tag: '20b',
          name: 'gpt-oss',
          description:
            'OpenAI’s open-weight models designed for powerful reasoning, agentic tasks, and versatile developer use cases.',
          contextLength: 128000,
          quality: OllamaModelQuality.Good,
          speed: OllamaModelSpeed.Fast,
          capabilities: [
            OllamaModelCapability.TextGeneration,
            OllamaModelCapability.Thinking,
            OllamaModelCapability.ToolCalling,
          ],
          size: 14,
          fullName: '',
          provider: ModelProvider.OpenAI,
          platforms: ['macos'],
        },
        {
          model: 'mistral-small3.2',
          tag: '24b-instruct-2506-q4_K_M',
          name: 'Mistral Small 3.2',
          description:
            'An update to Mistral Small that improves function calling, instruction following, and reduces repetition errors.',
          contextLength: 128000,
          quality: OllamaModelQuality.Medium,
          speed: OllamaModelSpeed.Fast,
          capabilities: [
            OllamaModelCapability.TextGeneration,
            OllamaModelCapability.ImageToText,
            OllamaModelCapability.ToolCalling,
          ],
          size: 15,
          fullName: '',
          provider: ModelProvider.Mistral,
          platforms: ['macos'],
        },
      ]
    : []),
  {
    model: 'qwen3',
    tag: '30b-a3b',
    name: 'Qwen 3 30B-A3B',
    description:
      'Qwen 3 30B-A3B is a mixture-of-experts (MoE) model with 30B total parameters and 3B active parameters. It features seamless switching between thinking and non-thinking modes, excelling at complex reasoning, math, coding, and general dialogue while being highly efficient.',
    contextLength: 128000,
    quality: OllamaModelQuality.Good,
    speed: OllamaModelSpeed.Fast,
    capabilities: [
      OllamaModelCapability.TextGeneration,
      OllamaModelCapability.Thinking,
      OllamaModelCapability.ToolCalling,
    ],
    size: 15.6,
    fullName: '',
    provider: ModelProvider.Qwen,
    platforms: ['windows', 'linux', 'macos'],
  },
  {
    model: 'deepseek-r1',
    tag: '70b',
    name: 'DeepSeek R1 70B',
    description:
      'DeepSeek R1 70B is a powerful reasoning model achieving performance comparable to OpenAI-o1 across math, code, and reasoning tasks. It is derived from Llama3.3-70B-Instruct and optimized through distillation from larger models.',
    contextLength: 128000,
    quality: OllamaModelQuality.Good,
    speed: OllamaModelSpeed.Average,
    capabilities: [
      OllamaModelCapability.TextGeneration,
      OllamaModelCapability.Thinking,
      OllamaModelCapability.ToolCalling,
    ],
    size: 40.2,
    fullName: '',
    provider: ModelProvider.DeepSeek,
    platforms: ['windows', 'linux', 'macos'],
  },
].map((model) => {
  model.fullName = `${model.model}:${model.tag}` as const;
  return model;
});
