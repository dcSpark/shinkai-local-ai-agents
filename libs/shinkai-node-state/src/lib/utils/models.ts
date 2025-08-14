export enum Models {
  OpenAI = 'open-ai',
  TogetherComputer = 'togethercomputer',
  Ollama = 'ollama',
  Gemini = 'gemini',
  Exo = 'exo',
  Groq = 'groq',
  OpenRouter = 'openrouter',
  Claude = 'claude',
  DeepSeek = 'deepseek',
  Grok = 'grok',
}

export const modelsConfig = {
  [Models.OpenAI]: {
    apiUrl: 'https://api.openai.com',
    modelTypes: [
      {
        name: 'GPT 5',
        value: 'gpt-5',
      },
      {
        name: 'GPT 5 Mini',
        value: 'gpt-5-mini',
      },
      {
        name: 'GPT 5 Nano',
        value: 'gpt-5-nano',
      },
      {
        name: 'GPT 4o Mini',
        value: 'gpt-4o-mini',
      },
      {
        name: 'GPT 4o',
        value: 'gpt-4o',
      },
      {
        name: 'GPT 4-1106 Preview',
        value: 'gpt-4-1106-preview',
      },
      {
        name: 'GPT 4 Vision Preview',
        value: 'gpt-4-vision-preview',
      },
      {
        name: 'GPT 3.5 Turbo 1106',
        value: 'gpt-3.5-turbo-1106',
      },
      {
        name: 'GPT 4.1',
        value: 'gpt-4.1',
      },
      {
        name: 'GPT 4.1 Mini',
        value: 'gpt-4.1-mini',
      },
      {
        name: 'GPT 4.1 Nano',
        value: 'gpt-4.1-nano',
      },
      {
        name: '4o Preview',
        value: '4o-preview',
      },
      {
        name: '4o Mini',
        value: '4o-mini',
      },
      {
        name: 'o1',
        value: 'o1',
      },
      {
        name: 'o1 Mini',
        value: 'o1-mini',
      },
      {
        name: 'o3 Mini',
        value: 'o3-mini',
      },
    ],
  },
  [Models.TogetherComputer]: {
    apiUrl: 'https://api.together.xyz',
    modelTypes: [
      {
        name: 'Meta - LLaMA-2 Chat (70B)',
        value: 'togethercomputer/llama-2-70b-chat',
      },
      {
        name: 'mistralai - Mistral (7B) Instruct',
        value: 'mistralai/Mistral-7B-Instruct-v0.1',
      },
      {
        name: 'teknium - OpenHermes-2-Mistral (7B)',
        value: 'teknium/OpenHermes-2-Mistral-7B',
      },
      {
        name: 'OpenOrca - OpenOrca Mistral (7B) 8K',
        value: 'Open-Orca/Mistral-7B-OpenOrca',
      },
    ],
  },
  [Models.Ollama]: {
    apiUrl: 'http://localhost:11434',
    modelTypes: [
      {
        name: 'Llama 2',
        value: 'llama2',
      },
      {
        name: 'Mistral',
        value: 'mistral',
      },
      {
        name: 'Mixtral',
        value: 'mixtral',
      },
    ],
  },
  [Models.Gemini]: {
    apiUrl: 'https://generativelanguage.googleapis.com/v1beta/models',
    modelTypes: [
      {
        name: 'Gemini 2.5 Pro',
        value: 'gemini-2.5-pro',
      },
      {
        name: 'Gemini 2.5 Flash',
        value: 'gemini-2.5-flash',
      },
      {
        name: 'Gemini 2.5 Flash-Lite',
        value: 'gemini-2.5-flash-lite',
      },
      {
        name: 'Gemini 2.5 Flash Native Audio',
        value: 'gemini-2.5-flash-preview-native-audio-dialog',
      },
      {
        name: 'Gemini 2.5 Flash Exp Native Audio',
        value: 'gemini-2.5-flash-exp-native-audio-thinking-dialog',
      },
      {
        name: 'Gemini 2.5 Flash Preview TTS',
        value: 'gemini-2.5-flash-preview-tts',
      },
      {
        name: 'Gemini 2.5 Pro Preview TTS',
        value: 'gemini-2.5-pro-preview-tts',
      },
      {
        name: 'Gemini 2.0 Flash',
        value: 'gemini-2.0-flash',
      },
      {
        name: 'Gemini 2.0 Flash Preview Image Generation',
        value: 'gemini-2.0-flash-preview-image-generation',
      },
      {
        name: 'Gemini 2.0 Flash-Lite',
        value: 'gemini-2.0-flash-lite',
      },
      {
        name: 'Gemini 2.0 Flash Live',
        value: 'gemini-2.0-flash-live-001',
      },
      {
        name: 'Gemini 1.5 Flash',
        value: 'gemini-1.5-flash',
      },
      {
        name: 'Gemini 1.5 Flash-8B',
        value: 'gemini-1.5-flash-8b',
      },
      {
        name: 'Gemini 1.5 Pro',
        value: 'gemini-1.5-pro',
      },
      {
        name: 'Gemini Embedding',
        value: 'gemini-embedding-exp',
      },
      {
        name: 'Imagen 3',
        value: 'imagen-3.0-generate-002',
      },
      {
        name: 'Veo 2',
        value: 'veo-2.0-generate-001',
      },
    ],
  },
  [Models.Exo]: {
    apiUrl: 'http://localhost:8000',
    modelTypes: [
      {
        name: 'Llama3.1-8b',
        value: 'llama3.1-8b',
      },
    ],
  },
  [Models.Groq]: {
    apiUrl: 'https://api.groq.com/openai/v1',
    modelTypes: [
      {
        name: 'Llama 3.2 90B Vision (Preview)',
        value: 'llama-3.2-90b-vision-preview',
      },
      {
        name: 'Llama 3.2 11B Vision (Preview)',
        value: 'llama-3.2-11b-vision-preview',
      },
      {
        name: 'Llama3 Groq 70B (Tool Use Preview)',
        value: 'llama3-groq-70b-8192-tool-use-preview',
      },
      {
        name: 'Llama3 Groq 8B (Tool Use Preview)',
        value: 'llama3-groq-8b-8192-tool-use-preview',
      },
      {
        name: 'Llama 3.1 70B Versatile',
        value: 'llama-3.1-70b-versatile',
      },
      {
        name: 'Llama 3.1 8B Instant',
        value: 'llama-3.1-8b-instant',
      },
      {
        name: 'Qwen-QWQ-32B',
        value: 'qwen-qwq-32b',
      },
      {
        name: 'Gemma2 9B IT',
        value: 'gemma2-9b-it',
      },
      {
        name: 'Llama Guard 4 12B',
        value: 'meta-llama/llama-guard-4-12b',
      },
      {
        name: 'Llama3 70B 8192',
        value: 'llama3-70b-8192',
      },
      {
        name: 'Llama3 8B 8192',
        value: 'llama3-8b-8192',
      },
      {
        name: 'Allam 2 7B',
        value: 'allam-2-7b',
      },
      {
        name: 'DeepSeek Distill Llama 70B',
        value: 'deepseek-r1-distill-llama-70b',
      },
      {
        name: 'Llama 4 Maverick 17B Instruct',
        value: 'meta-llama/llama-4-maverick-17b-128e-instruct',
      },
      {
        name: 'Llama 4 Scout 17B Instruct',
        value: 'meta-llama/llama-4-scout-17b-16e-instruct',
      },
      {
        name: 'Llama Prompt Guard 2 22M',
        value: 'meta-llama/llama-prompt-guard-2-22m',
      },
      {
        name: 'Llama Prompt Guard 2 86M',
        value: 'meta-llama/llama-prompt-guard-2-86m',
      },
      {
        name: 'Mistral Saba 24B',
        value: 'mistral-saba-24b',
      },
    ],
  },
  [Models.OpenRouter]: {
    apiUrl: 'https://openrouter.ai',
    modelTypes: [],
  },
  [Models.Claude]: {
    apiUrl: 'https://api.anthropic.com',
    modelTypes: [
      {
        name: 'Claude 4.1 Opus',
        value: 'claude-opus-4-1',
      },
      {
        name: 'Claude 4 Opus',
        value: 'claude-opus-4-0',
      },      
      {
        name: 'Claude 4 Sonnet',
        value: 'claude-sonnet-4-0',
      },
      {
        name: 'Claude 3.7 Sonnet',
        value: 'claude-3-7-sonnet-latest',
      },
      {
        name: 'Claude 3.5 Haiku',
        value: 'claude-3-5-haiku-latest',
      },
    ],
  },
  [Models.DeepSeek]: {
    apiUrl: 'https://api.deepseek.com',
    modelTypes: [
      {
        name: 'DeepSeek Chat',
        value: 'deepseek-chat',
      },
      {
        name: 'DeepSeek Reasoner',
        value: 'deepseek-reasoner',
      },
    ],
  },
  [Models.Grok]: {
    apiUrl: 'https://api.x.ai',
    modelTypes: [
      {
        name: 'Grok 4',
        value: 'grok-4',
      },      
      {
        name: 'Grok 3',
        value: 'grok-3',
      },
      {
        name: 'Grok 3 Mini',
        value: 'grok-3-mini',
      },
      {
        name: 'Grok 3 Fast',
        value: 'grok-3-fast',
      },
      {
        name: 'Grok 3 Mini Fast',
        value: 'grok-3-mini-fast',
      },
      {
        name: 'Grok 2 Vision',
        value: 'grok-2-vision',
      },
    ],
  },
};
