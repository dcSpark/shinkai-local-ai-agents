import { type LocaleMode, switchLanguage } from '@shinkai_network/shinkai-i18n';
import { create } from 'zustand';
import { devtools, persist } from 'zustand/middleware';

import {
  ONBOARDING_STEPS,
  type OnboardingStep,
  type OnboardingStepConfig,
  type StepChoiceMap,
  validateChoice,
  type OnboardingState,
} from '../components/onboarding/constants';

export enum TutorialBanner {
  SHINKAI_TOOLS = 'shinkai-tools',
  FILES_EXPLORER = 'files-explorer',
  SCHEDULED_TASKS = 'scheduled-tasks',
  AIS_AND_AGENTS = 'ais-and-agents',
}

export type ChatFontSize = 'sm' | 'base' | 'lg' | 'xl';

type SettingsStore = {
  onboarding: OnboardingState;
  completeStep: <T extends OnboardingStep>(
    stepId: T,
    choice?: StepChoiceMap[T],
  ) => void;
  getNextStep: () => OnboardingStepConfig | null;
  isOnboardingComplete: () => boolean;
  isStepCompleted: (stepId: OnboardingStep) => boolean;
  getStepChoice: <T extends OnboardingStep>(stepId: T) => StepChoiceMap[T];
  getStepById: (stepId: OnboardingStep) => OnboardingStepConfig | undefined;
  getStepByPath: (path: string) => OnboardingStepConfig | undefined;
  getCurrentStep: () => number | null;
  updateStepChoice: <T extends OnboardingStep>(
    stepId: T,
    choice: StepChoiceMap[T],
  ) => void;

  defaultAgentId: string;
  setDefaultAgentId: (defaultAgentId: string) => void;
  defaultSpotlightAiId: string;
  setDefaultSpotlightAiId: (defaultSpotlightAiId: string) => void;
  sidebarExpanded: boolean;
  toggleSidebar: () => void;
  isGetStartedChecklistHidden: boolean;
  setGetStartedChecklistHidden: (
    isGetStartedChecklistHidden: boolean | undefined,
  ) => void;
  optInExperimental?: boolean;
  setOptInExperimental: (optInExperimental: boolean) => void;
  userLanguage: LocaleMode;
  setUserLanguage: (userLanguage: LocaleMode) => void;
  evmAddress: string;
  setEvmAddress: (evmAddress: string) => void;
  heightRow: 'small' | 'medium' | 'large' | 'extra-large';
  setHeightRow: (height: 'small' | 'medium' | 'large' | 'extra-large') => void;
  compatibilityBannerDismissed: boolean;
  setCompatibilityBannerDismissed: (dismissed: boolean) => void;
  isChatSidebarCollapsed: boolean;
  setChatSidebarCollapsed: (isChatSidebarCollapsed: boolean) => void;
  chatFontSize: ChatFontSize;
  getChatFontSizeInPts: () => number;
  setChatFontSize: (size: ChatFontSize) => void;
  resetSettings: () => void;
  dismissedCommunityAgentsDisclaimer: boolean;
  setDismissedCommunityAgentsDisclaimer: (dismissed: boolean) => void;
  dismissedTutorialBanners: TutorialBanner[];
  dismissTutorialBanner: (bannerId: TutorialBanner) => void;
  embeddingModelMismatchPromptDismissed: boolean;
  setEmbeddingModelMismatchPromptDismissed: (dismissed: boolean) => void;
  // Playground Tool
  playgroundChatPanelSize: number;
  setPlaygroundChatPanelSize: (size: number) => void;
  playgroundCodePanelSize: number;
  setPlaygroundCodePanelSize: (size: number) => void;
  // legacy state
  termsAndConditionsAccepted?: boolean;
  getTermsAndConditionsAccepted: () => boolean | undefined;
};

export const useSettings = create<SettingsStore>()(
  devtools(
    persist(
      (set, get) => ({
        onboarding: {
          steps: ONBOARDING_STEPS.reduce(
            (acc, step) => ({
              ...acc,
              [step.id]: {
                completed: false,
                choice: null,
              },
            }),
            {} as OnboardingState['steps'],
          ),
        },

        completeStep: <T extends OnboardingStep>(
          stepId: T,
          choice: StepChoiceMap[T] = null,
        ) => {
          set((state) => {
            const validatedChoice = validateChoice(stepId, choice);

            const updatedSteps = {
              ...state.onboarding.steps,
              [stepId]: {
                completed: true,
                choice: validatedChoice,
              },
            };

            return {
              onboarding: {
                ...state.onboarding,
                steps: updatedSteps,
              },
            };
          });
        },
        getStepById: (stepId) => {
          return ONBOARDING_STEPS.find((step) => step.id === stepId);
        },
        getStepByPath: (path) => {
          return ONBOARDING_STEPS.find((step) => step.path === path);
        },
        updateStepChoice: (stepId, choice) => {
          set((state) => ({
            onboarding: {
              ...state.onboarding,
              steps: {
                ...state.onboarding.steps,
                [stepId]: {
                  ...state.onboarding.steps[stepId],
                  choice,
                },
              },
            },
          }));
        },
        getNextStep: () => {
          const { steps } = get().onboarding;
          return (
            ONBOARDING_STEPS.find(
              (step) => step.required && !steps[step.id]?.completed,
            ) || null
          );
        },
        isStepCompleted: (stepId) => {
          return get().onboarding.steps[stepId]?.completed || false;
        },
        getStepChoice: (stepId) => {
          return get().onboarding.steps[stepId]?.choice || null;
        },
        getCurrentStep: () => {
          const { steps } = get().onboarding;
          const currentStep = ONBOARDING_STEPS.find(
            (step) => step.required && !steps[step.id]?.completed,
          );
          return currentStep ? ONBOARDING_STEPS.indexOf(currentStep) + 1 : null;
        },
        isOnboardingComplete: () => {
          const { steps } = get().onboarding;
          return ONBOARDING_STEPS.filter((step) => step.required).every(
            (step) => steps[step.id]?.completed,
          );
        },

        defaultAgentId: '',
        setDefaultAgentId: (defaultAgentId) => {
          set({ defaultAgentId });
        },

        defaultSpotlightAiId: '',
        setDefaultSpotlightAiId: (defaultSpotlightAiId) => {
          set({ defaultSpotlightAiId });
        },

        sidebarExpanded: false,
        toggleSidebar: () => {
          set((state) => ({ sidebarExpanded: !state.sidebarExpanded }));
        },

        isGetStartedChecklistHidden: false,
        setGetStartedChecklistHidden: (isGetStartedChecklistHidden) => {
          set({ isGetStartedChecklistHidden });
        },

        optInExperimental: false,
        setOptInExperimental: (optInExperimental) => {
          set({ optInExperimental });
        },

        userLanguage: 'auto',
        setUserLanguage: (userLanguage) => {
          set({ userLanguage });
          switchLanguage(userLanguage);
        },

        evmAddress: '',
        setEvmAddress: (evmAddress) => {
          set({ evmAddress });
        },

        heightRow: 'large',
        setHeightRow: (heightRow) => {
          set({ heightRow });
        },

        compatibilityBannerDismissed: false,
        setCompatibilityBannerDismissed: (dismissed) => {
          set({ compatibilityBannerDismissed: dismissed });
        },

        isChatSidebarCollapsed: false,
        setChatSidebarCollapsed: (isChatSidebarCollapsed) => {
          set({ isChatSidebarCollapsed });
        },

        chatFontSize: 'base',
        getChatFontSizeInPts: () => {
          const { chatFontSize } = get();
          switch (chatFontSize) {
            case 'sm':
              return 14;
            case 'base':
              return 16;
            case 'lg':
              return 18;
            case 'xl':
              return 20;
            default:
              return 16;
          }
        },
        setChatFontSize: (size) => {
          set({ chatFontSize: size });
        },

        dismissedCommunityAgentsDisclaimer: false,
        setDismissedCommunityAgentsDisclaimer: (dismissed) => {
          set({ dismissedCommunityAgentsDisclaimer: dismissed });
        },

        embeddingModelMismatchPromptDismissed: false,
        setEmbeddingModelMismatchPromptDismissed: (dismissed) => {
          set({ embeddingModelMismatchPromptDismissed: dismissed });
        },

        dismissedTutorialBanners: [] as TutorialBanner[],
        dismissTutorialBanner: (bannerId: TutorialBanner) => {
          set((state) => ({
            dismissedTutorialBanners: Array.from(
              new Set([...state.dismissedTutorialBanners, bannerId]),
            ),
          }));
        },

        playgroundChatPanelSize: 40,
        setPlaygroundChatPanelSize: (size) => {
          set({ playgroundChatPanelSize: size });
        },
        playgroundCodePanelSize: 50,
        setPlaygroundCodePanelSize: (size) => {
          set({ playgroundCodePanelSize: size });
        },

        resetSettings: () => {
          set({
            onboarding: {
              steps: ONBOARDING_STEPS.reduce(
                (acc, step) => ({
                  ...acc,
                  [step.id]: {
                    completed: false,
                    choice: null,
                  },
                }),
                {},
              ),
            },
            defaultAgentId: '',
            defaultSpotlightAiId: '',
            sidebarExpanded: false,
            isGetStartedChecklistHidden: false,
            optInExperimental: false,
            userLanguage: 'auto',
            evmAddress: '',
            heightRow: 'large',
            compatibilityBannerDismissed: false,
            isChatSidebarCollapsed: false,
            chatFontSize: 'sm',
            termsAndConditionsAccepted: undefined,
          });
        },

        // legacy state
        termsAndConditionsAccepted: undefined,
        getTermsAndConditionsAccepted: () => {
          const state = get();
          return state.termsAndConditionsAccepted;
        },
      }),
      {
        name: 'settings',
      },
    ),
  ),
);
