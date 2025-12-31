import { type Artifact } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { type ReactNode, createContext, useContext, useState } from 'react';
import { createStore, useStore } from 'zustand';

export type ToolView = 'form' | 'raw';
export type ScrollFlag = ScrollBehavior | false;

type ChatStore = {
  selectedArtifact: Artifact | null;
  setSelectedArtifact: (selectedArtifact: Artifact | null) => void;
  // tool preview (form or raw)
  chatToolView: ToolView;
  setChatToolView: (chatToolView: ToolView) => void;
  toolRawInput: string;
  setToolRawInput: (toolRawInput: string) => void;
  // quoted text from message selection
  quotedText: string | null;
  setQuotedText: (quotedText: string | null) => void;
  // scroll management
  scrollBehavior: ScrollFlag;
  setScrollBehavior: (scrollBehavior: ScrollFlag) => void;
  autoScroll: boolean;
  setAutoScroll: (autoScroll: boolean) => void;
};

const createChatStore = () =>
  createStore<ChatStore>((set) => ({
    selectedArtifact: null,
    setSelectedArtifact: (selectedArtifact: Artifact | null) =>
      set({ selectedArtifact }),

    chatToolView: 'form',
    setChatToolView: (chatToolView: ToolView) => set({ chatToolView }),

    toolRawInput: '',
    setToolRawInput: (toolRawInput: string) => set({ toolRawInput }),

    quotedText: null,
    setQuotedText: (quotedText: string | null) => set({ quotedText }),

    scrollBehavior: false,
    setScrollBehavior: (scrollBehavior: ScrollFlag) => set({ scrollBehavior }),

    autoScroll: true,
    setAutoScroll: (autoScroll: boolean) => set({ autoScroll }),
  }));

const ChatContext = createContext<ReturnType<typeof createChatStore> | null>(
  null,
);

export const ChatProvider = ({ children }: { children: ReactNode }) => {
  const [store] =
    useState<ReturnType<typeof createChatStore>>(createChatStore());

  return <ChatContext.Provider value={store}>{children}</ChatContext.Provider>;
};

export function useChatStore<T>(selector: (state: ChatStore) => T) {
  const store = useContext(ChatContext);
  if (!store) {
    throw new Error('Missing ChatProvider');
  }
  const value = useStore(store, selector);
  return value;
}
