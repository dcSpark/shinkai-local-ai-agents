import { MODELS_WITH_THINKING_SUPPORT } from './constants';

export interface ThinkingConfig {
  supportsThinking: boolean;
  forceEnabled: boolean;
  reasoningLevel: boolean;
}

/**
 * Utility function for thinking model detection
 * Checks if a model supports thinking capabilities and returns configuration
 */
export const getThinkingConfig = (modelName: string | undefined): ThinkingConfig => {
  if (!modelName) {
    return { supportsThinking: false, forceEnabled: false, reasoningLevel: false };
  }

  const currentModel = modelName.toLowerCase();
  const supportedModel = Object.keys(MODELS_WITH_THINKING_SUPPORT).find(
    (supportedModel) => {
      const normalizedSupported = supportedModel.toLowerCase();
      return currentModel === normalizedSupported || currentModel.includes(normalizedSupported);
    }
  );

  if (!supportedModel) {
    return { supportsThinking: false, forceEnabled: false, reasoningLevel: false };
  }

  const config =
    MODELS_WITH_THINKING_SUPPORT[
      supportedModel as keyof typeof MODELS_WITH_THINKING_SUPPORT
    ];
  return {
    supportsThinking: true,
    forceEnabled: config.forceEnabled,
    reasoningLevel: config.reasoningLevel,
  };
};
