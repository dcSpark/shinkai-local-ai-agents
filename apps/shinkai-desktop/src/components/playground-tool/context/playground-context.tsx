import { type ToolMetadata } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { type PrismEditor } from 'prism-react-editor';
import { createContext, createRef, useContext, useState } from 'react';
import { createStore, useStore } from 'zustand';

type Status = 'idle' | 'pending' | 'success' | 'error';

export type ToolCreationStep =
  | 'initial-prompt'
  | 'feedback-required'
  | 'generating-code'
  | 'generating-metadata'
  | 'saving'
  | 'completed'
  | 'error';

export enum ToolCreationState {
  PROMPT_INPUT = 'PROMPT_INPUT',
  PLAN_REVIEW = 'PLAN_REVIEW',
  CREATING_CODE = 'CREATING_CODE',
  CREATING_METADATA = 'CREATING_METADATA',
  SAVING_TOOL = 'SAVING_TOOL',
  COMPLETED = 'COMPLETED',
}

type PlaygroundStore = {
  // steps
  currentStep: ToolCreationState;
  setCurrentStep: (currentStep: ToolCreationState) => void;
  toolCreationError: string | null;
  setToolCreationError: (toolCreationError: string | null) => void;
  // inboxId
  chatInboxId: string | undefined;
  setChatInboxId: (chatInboxId: string | undefined) => void;
  metadataInboxId: string | undefined;
  setMetadataInboxId: (metadataInboxId: string | undefined) => void;
  // code
  toolCodeStatus: Status;
  setToolCodeStatus: (
    toolCodeStatus: Status | ((prev: Status) => Status),
  ) => void;
  toolCode: string;
  setToolCode: (toolCode: string) => void;
  toolCodeError: string | null;
  setToolCodeError: (toolCodeError: string | null) => void;
  // metadata
  toolMetadataStatus: Status;
  setToolMetadataStatus: (toolMetadataStatus: Status) => void;
  toolMetadata: ToolMetadata | null;
  setToolMetadata: (toolMetadata: ToolMetadata | null) => void;
  toolMetadataError: string | null;
  setToolMetadataError: (toolMetadataError: string | null) => void;
  updateToolMetadata: (metadata: ToolMetadata) => void;

  // execution result
  toolResult: object | null;
  setToolResult: (toolResult: object | null) => void;
  // reset counter for updating tool code
  resetCounter: number;
  setResetCounter: (resetCounter: number | ((prev: number) => number)) => void;

  xShinkaiAppId: string;
  xShinkaiToolId: string;

  focusedPanel: 'code' | 'metadata' | 'console' | 'preview' | null;
  setFocusedPanel: (
    focusedPanel: 'code' | 'metadata' | 'console' | 'preview' | null,
  ) => void;

  resetPlaygroundStore: () => void;

  metadataEditorRef: React.RefObject<PrismEditor | null>;
  codeEditorRef: React.RefObject<PrismEditor | null>;
  forceGenerateCode: React.RefObject<boolean | null>;
  forceGenerateMetadata: React.RefObject<boolean | null>;
  forceAutoSave: React.RefObject<boolean | null>;
};

const createPlaygroundStore = () =>
  createStore<PlaygroundStore>((set) => ({
    // steps
    currentStep: ToolCreationState.PROMPT_INPUT,
    setCurrentStep: (currentStep) => set({ currentStep }),
    toolCreationError: null,
    setToolCreationError: (toolCreationError) => set({ toolCreationError }),
    // inboxId
    chatInboxId: undefined,
    setChatInboxId: (chatInboxId) => set({ chatInboxId }),
    metadataInboxId: undefined,
    setMetadataInboxId: (metadataInboxId) => set({ metadataInboxId }),
    // code
    toolCodeStatus: 'idle',
    setToolCodeStatus: (toolCodeStatus) =>
      set((state) => ({
        toolCodeStatus:
          typeof toolCodeStatus === 'function'
            ? toolCodeStatus(state.toolCodeStatus)
            : toolCodeStatus,
      })),
    toolCode: '',
    setToolCode: (toolCode) => set({ toolCode }),
    toolCodeError: null,
    setToolCodeError: (toolCodeError) => set({ toolCodeError }),
    // metadata
    toolMetadataStatus: 'idle',
    setToolMetadataStatus: (toolMetadataStatus) => set({ toolMetadataStatus }),
    toolMetadata: null,
    setToolMetadata: (toolMetadata) => set({ toolMetadata }),
    toolMetadataError: null,
    setToolMetadataError: (toolMetadataError) => set({ toolMetadataError }),
    updateToolMetadata: (toolMetadata) =>
      set((_state) => ({
        toolMetadata,
        toolMetadataStatus: 'success',
        toolMetadataError: null,
      })),
    // execution result
    toolResult: null,
    setToolResult: (toolResult) => set({ toolResult }),
    // reset counter for updating tool code
    resetCounter: 0,
    setResetCounter: (resetCounter) =>
      set((state) => ({
        resetCounter:
          typeof resetCounter === 'function'
            ? resetCounter(state.resetCounter)
            : resetCounter,
      })),

    xShinkaiAppId: `app-id-${Date.now()}`,
    xShinkaiToolId: `task-id-${Date.now()}`,

    focusedPanel: 'code',
    setFocusedPanel: (focusedPanel) => set({ focusedPanel }),

    metadataEditorRef: createRef<PrismEditor>(),
    codeEditorRef: createRef<PrismEditor>(),
    forceGenerateCode: createRef<boolean>(),
    forceGenerateMetadata: createRef<boolean>(),
    forceAutoSave: createRef<boolean>(),
    resetPlaygroundStore: () =>
      set({
        currentStep: ToolCreationState.PROMPT_INPUT,
        chatInboxId: undefined,
        metadataInboxId: undefined,
        toolCodeStatus: 'idle',
        toolCode: '',
        toolCodeError: null,
        toolMetadataStatus: 'idle',
        toolMetadata: null,
        toolMetadataError: null,
        toolResult: null,
        resetCounter: 0,
        focusedPanel: 'code',
        xShinkaiAppId: `app-id-${Date.now()}`,
        xShinkaiToolId: `task-id-${Date.now()}`,
        forceGenerateCode: createRef<boolean>(),
        forceGenerateMetadata: createRef<boolean>(),
        codeEditorRef: createRef<PrismEditor>(),
        metadataEditorRef: createRef<PrismEditor>(),
        forceAutoSave: createRef<boolean>(),
      }),
  }));

const PlaygroundContext = createContext<ReturnType<
  typeof createPlaygroundStore
> | null>(null);

export const PlaygroundProvider = ({
  children,
}: {
  children: React.ReactNode;
}) => {
  const [store] = useState<ReturnType<typeof createPlaygroundStore>>(() =>
    createPlaygroundStore(),
  );

  return (
    <PlaygroundContext.Provider value={store}>
      {children}
    </PlaygroundContext.Provider>
  );
};

export function usePlaygroundStore<T>(selector: (state: PlaygroundStore) => T) {
  const store = useContext(PlaygroundContext);
  if (!store) {
    throw new Error('Missing PlaygroundProvider');
  }
  const value = useStore(store, selector);
  return value;
}
