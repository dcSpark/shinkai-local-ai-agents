import { type FormProps } from '@rjsf/core';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type ToolMetadata } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import {
  buildInboxIdFromJobId,
  extractJobIdFromInbox,
} from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { useCreateToolCode } from '@shinkai_network/shinkai-node-state/v2/mutations/createToolCode/useCreateToolCode';
import { useCreateToolMetadata } from '@shinkai_network/shinkai-node-state/v2/mutations/createToolMetadata/useCreateToolMetadata';
import { useExecuteToolCode } from '@shinkai_network/shinkai-node-state/v2/mutations/executeToolCode/useExecuteToolCode';
import { useUpdateToolCodeImplementation } from '@shinkai_network/shinkai-node-state/v2/mutations/updateToolCodeImplementation/useUpdateToolCodeImplementation';
import { debug } from '@tauri-apps/plugin-log';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { type UseFormReturn } from 'react-hook-form';
import { useLocation, useNavigate } from 'react-router';
import { toast } from 'sonner';

import { handleSendNotification } from '../../../lib/notifications';
import { useAnalytics } from '../../../lib/posthog-provider';
import { useChatConversationWithOptimisticUpdates } from '../../../pages/chat/chat-conversation';
import { useAuth } from '../../../store/auth';
import { useWebSocketMessage } from '../../chat/websocket-message';
import {
  ToolCreationState,
  usePlaygroundStore,
} from '../context/playground-context';
import { ToolMetadataSchema, type ToolMetadataSchemaType } from '../schemas';
import {
  extractCodeByLanguage,
  extractCodeLanguage,
  parseJsonFromCodeBlock,
  validateCodeSnippet,
} from '../utils/code';
import { type CreateToolCodeFormSchema } from './use-tool-code';
import { useToolSave } from './use-tool-save';

export type ToolCreationStepStatus = 'idle' | 'pending' | 'success' | 'error';

export interface ToolFlowOptions {
  form: UseFormReturn<CreateToolCodeFormSchema>;
  initialInboxId?: string;
  initialState?: {
    code?: string;
    metadata?: ToolMetadata | null;
    codeState?: ToolCreationStepStatus;
    metadataState?: ToolCreationStepStatus;
    error?: string | null;
  };
  requireFeedbackFlow?: boolean;
  isPlaygroundMode?: boolean;
  tools?: string[];
}

export const useToolFlow = ({
  form,
  initialInboxId,
  initialState,
  requireFeedbackFlow = false,
  isPlaygroundMode = false,
  tools,
}: ToolFlowOptions) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();

  const currentStep = usePlaygroundStore((state) => state.currentStep);
  const setCurrentStep = usePlaygroundStore((state) => state.setCurrentStep);
  const toolCreationError = usePlaygroundStore(
    (state) => state.toolCreationError,
  );

  const setToolCreationError = usePlaygroundStore(
    (state) => state.setToolCreationError,
  );

  const location = useLocation();
  const isFeedbackPage = location.pathname.includes('/tool-feedback');

  const forceGenerateCode = usePlaygroundStore(
    (state) => state.forceGenerateCode,
  );
  const forceGenerateMetadata = usePlaygroundStore(
    (state) => state.forceGenerateMetadata,
  );
  const forceAutoSave = usePlaygroundStore((state) => state.forceAutoSave);

  const baseToolCodeRef = useRef<string>(initialState?.code || '');
  const codeEditorRef = usePlaygroundStore((state) => state.codeEditorRef);
  const metadataEditorRef = usePlaygroundStore(
    (state) => state.metadataEditorRef,
  );

  const [isDirtyCodeEditor, setIsDirtyCodeEditor] = useState(false);
  const resetCounter = usePlaygroundStore((state) => state.resetCounter);
  const setResetCounter = usePlaygroundStore((state) => state.setResetCounter);

  const chatInboxId = usePlaygroundStore((state) => state.chatInboxId);
  const setChatInboxId = usePlaygroundStore((state) => state.setChatInboxId);
  const toolCode = usePlaygroundStore((state) => state.toolCode);

  const setToolCode = usePlaygroundStore((state) => state.setToolCode);
  const toolCodeStatus = usePlaygroundStore((state) => state.toolCodeStatus);
  const setToolCodeStatus = usePlaygroundStore(
    (state) => state.setToolCodeStatus,
  );
  const toolCodeError = usePlaygroundStore((state) => state.toolCodeError);
  const setToolCodeError = usePlaygroundStore(
    (state) => state.setToolCodeError,
  );

  const toolResult = usePlaygroundStore((state) => state.toolResult);
  const setToolResult = usePlaygroundStore((state) => state.setToolResult);

  const toolMetadata = usePlaygroundStore((state) => state.toolMetadata);
  const setToolMetadata = usePlaygroundStore((state) => state.setToolMetadata);
  const toolMetadataStatus = usePlaygroundStore(
    (state) => state.toolMetadataStatus,
  );
  const setToolMetadataStatus = usePlaygroundStore(
    (state) => state.setToolMetadataStatus,
  );
  const toolMetadataError = usePlaygroundStore(
    (state) => state.toolMetadataError,
  );
  const setToolMetadataError = usePlaygroundStore(
    (state) => state.setToolMetadataError,
  );

  const xShinkaiAppId = usePlaygroundStore((state) => state.xShinkaiAppId);
  const xShinkaiToolId = usePlaygroundStore((state) => state.xShinkaiToolId);

  const mountTimestamp = useRef(new Date());
  const notificationSentRef = useRef(false);
  const feedbackNotificationSentRef = useRef(false);

  const { captureAnalyticEvent } = useAnalytics();

  // Helper function to send notification for feedback
  const sendFeedbackNotification = useCallback(() => {
    if (feedbackNotificationSentRef.current) {
      void debug('feedback notification already sent');
      return;
    }
    void debug('sending feedback notification');
    feedbackNotificationSentRef.current = true;
    void handleSendNotification(
      t('tools.notifications.feedbackRequired.title'),
      t('tools.notifications.feedbackRequired.description'),
    );
  }, [t]);

  // Send notifications for generation completion
  const sendCodeGenerationCompleteNotification = useCallback(() => {
    if (notificationSentRef.current) {
      void debug('code generation complete notification already sent');
      return;
    }
    void debug('sending code generation complete notification');
    notificationSentRef.current = true;
    void handleSendNotification(
      t('tools.notifications.codeGenerationComplete.title'),
      t('tools.notifications.codeGenerationComplete.description'),
    );
  }, [t]);

  const sendMetadataGenerationCompleteNotification = useCallback(() => {
    if (notificationSentRef.current) {
      void debug('metadata generation complete notification already sent');
      return;
    }
    void debug('sending metadata generation complete notification');
    notificationSentRef.current = true;
    void handleSendNotification(
      t('tools.notifications.metadataGenerationComplete.title'),
      t('tools.notifications.metadataGenerationComplete.description'),
    );
  }, [t]);

  useEffect(() => {
    if (initialState?.code) {
      baseToolCodeRef.current = initialState.code;
      setToolCode(initialState.code);
    }
    if (initialState?.codeState) {
      setToolCodeStatus(initialState.codeState);
    }
    if (initialState?.metadata) {
      setToolMetadata(initialState.metadata);
    }
    if (initialState?.metadataState) {
      setToolMetadataStatus(initialState.metadataState);
    }
  }, [
    initialState?.code,
    initialState?.codeState,
    initialState?.metadata,
    initialState?.metadataState,
    setToolCode,
    setToolCodeStatus,
    setToolMetadata,
    setToolMetadataStatus,
  ]);

  useEffect(() => {
    if (initialInboxId) {
      setChatInboxId(initialInboxId);
    }
  }, [initialInboxId, setChatInboxId]);

  const {
    data: conversationData,
    fetchPreviousPage,
    hasPreviousPage,
    isFetchingPreviousPage,
    isChatConversationLoading,
    isChatConversationSuccess,
  } = useChatConversationWithOptimisticUpdates({
    inboxId: chatInboxId ?? '',
  });

  useWebSocketMessage({ inboxId: chatInboxId ?? '', enabled: !!chatInboxId });

  const formattedConversationData = useMemo(() => {
    if (!conversationData) return;
    return {
      ...conversationData,
      pages: conversationData.pages?.map((page) =>
        page.map((message) => ({
          ...message,
          content:
            message.role === 'user'
              ? (message.content
                  .match(/<input_command>([\s\S]*?)<\/input_command>/)?.[1]
                  ?.trim() ?? '')
              : message.content,
        })),
      ),
    };
  }, [conversationData]);

  const metadataInboxId = usePlaygroundStore((state) => state.metadataInboxId);
  const setMetadataInboxId = usePlaygroundStore(
    (state) => state.setMetadataInboxId,
  );

  const { data: metadataConversation } =
    useChatConversationWithOptimisticUpdates({
      inboxId: metadataInboxId ?? '',
    });

  useWebSocketMessage({
    inboxId: metadataInboxId ?? '',
    enabled: !!metadataInboxId,
  });

  const toolHistory = useMemo(() => {
    const messageList = formattedConversationData?.pages.flat() ?? [];
    const currentLanguage = form.watch('language');

    const toolCodesFound = messageList
      .map((message) => {
        const language = extractCodeLanguage(message.content);
        if (
          message.role === 'assistant' &&
          message.status.type === 'complete' &&
          validateCodeSnippet(
            extractCodeByLanguage(
              message.content,
              language ?? currentLanguage,
            ) ?? '',
            currentLanguage,
          )
        ) {
          return {
            messageId: message.messageId,
            code:
              extractCodeByLanguage(
                message.content,
                language ?? currentLanguage,
              ) ?? '',
          };
        }
      })
      .filter((item) => !!item)
      .filter((item) => item.code);
    return toolCodesFound;
  }, [formattedConversationData?.pages, form]);

  const { mutateAsync: createToolCode, isPending: isCreatingToolCode } =
    useCreateToolCode({
      onSuccess: (data) => {
        setChatInboxId(data.inbox);
        setToolCode('');
        forceGenerateCode.current = true;
        baseToolCodeRef.current = '';
        setToolResult(null);
        executeToolCodeQuery.reset();
        if (isPlaygroundMode) return;
        if (isFeedbackPage) {
          setCurrentStep(ToolCreationState.PLAN_REVIEW);
        } else if (requireFeedbackFlow) {
          void navigate(`/tools/tool-feedback/${data.inbox}`, {
            state: {
              form: form.getValues(),
            },
          });
          setCurrentStep(ToolCreationState.PLAN_REVIEW);
        }
      },
      onError: (error) => {
        setToolCreationError(`Failed to create tool code: ${error.message}`);
      },
    });

  const { mutateAsync: createToolMetadata, isPending: isCreatingMetadata } =
    useCreateToolMetadata({
      onSuccess: (data) => {
        forceGenerateMetadata.current = false;
        setMetadataInboxId(buildInboxIdFromJobId(data.job_id));
      },
      onError: (error) => {
        setToolCreationError(
          `Failed to create tool metadata: ${error.message}`,
        );
        forceGenerateMetadata.current = false;
      },
    });

  const { mutateAsync: updateToolCodeImplementation } =
    useUpdateToolCodeImplementation();

  const executeToolCodeQuery = useExecuteToolCode({
    onSuccess: (data) => {
      setToolResult(data);
    },
  });

  const { handleSaveTool, isSavingTool } = useToolSave();

  useEffect(() => {
    const lastMessage = formattedConversationData?.pages?.at(-1)?.at(-1);
    if (!lastMessage || !forceGenerateCode.current) return;

    const isRunning =
      lastMessage.role === 'assistant' && lastMessage.status.type === 'running';

    if (isRunning) {
      setToolCodeStatus('pending');
      return;
    }

    if (
      lastMessage.role === 'assistant' &&
      lastMessage.status.type === 'complete'
    ) {
      const language = extractCodeLanguage(lastMessage.content);
      const currentLanguage = form.watch('language');

      const generatedCode = extractCodeByLanguage(
        lastMessage.content,
        language ?? currentLanguage,
      );

      if (
        generatedCode &&
        validateCodeSnippet(generatedCode, currentLanguage)
      ) {
        baseToolCodeRef.current = generatedCode;
        forceGenerateMetadata.current = true;

        setToolCode(generatedCode);
        setToolCodeStatus('success');
        sendCodeGenerationCompleteNotification();

        // if (!isPlaygroundMode) {
        setCurrentStep(ToolCreationState.CREATING_METADATA);
        // }
      } else {
        setToolCodeError('Failed to extract code from response');
        setToolCodeStatus('error');
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    formattedConversationData?.pages,
    form,
    setToolCode,
    setToolCodeError,
    forceGenerateCode,
    isPlaygroundMode,
    isFeedbackPage,
    forceGenerateMetadata,
    setToolCodeStatus,
    sendCodeGenerationCompleteNotification,
  ]);

  useEffect(() => {
    const lastMessage = metadataConversation?.pages?.at(-1)?.at(-1);
    if (!lastMessage) return;

    if (
      lastMessage.role === 'assistant' &&
      lastMessage.status.type === 'running'
    ) {
      if (toolMetadataStatus === 'pending') return;
      setToolMetadataStatus('pending');

      return;
    }

    if (
      lastMessage.role === 'assistant' &&
      lastMessage.status.type === 'complete'
    ) {
      const extractedMetadata = parseJsonFromCodeBlock(lastMessage.content);

      if (extractedMetadata) {
        try {
          const parsedMetadata = ToolMetadataSchema.parse(
            extractedMetadata,
          ) as ToolMetadataSchemaType;

          if (toolMetadataStatus === 'pending') {
            sendMetadataGenerationCompleteNotification();
          }
          setToolMetadata(parsedMetadata as ToolMetadata);
          setToolMetadataStatus('success');
          setToolMetadataError(null);

          if (!isPlaygroundMode) {
            setCurrentStep(ToolCreationState.SAVING_TOOL);
            forceAutoSave.current = true;
          }
        } catch (error) {
          setToolMetadataStatus('error');
          setToolMetadataError(
            error instanceof Error
              ? 'Invalid Metadata: ' + error.message
              : 'Invalid Metadata: Unknown Error',
          );
        }
      } else {
        setToolMetadataStatus('error');
        setToolMetadataError('No metadata JSON code found');

        if (!isPlaygroundMode) {
          setToolCreationError(
            'Failed to extract metadata from response. Metadata Response: ' +
              JSON.stringify(lastMessage.content),
          );
        }
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    metadataConversation?.pages,
    setToolMetadata,
    setToolMetadataStatus,
    setToolMetadataError,
    isPlaygroundMode,
    toolMetadataStatus,
    sendMetadataGenerationCompleteNotification,
    setCurrentStep,
    setToolCreationError,
  ]);

  useEffect(() => {
    const lastMessage = conversationData?.pages.flat().at(-1);
    if (
      (lastMessage &&
        isFeedbackPage &&
        currentStep === ToolCreationState.PLAN_REVIEW &&
        lastMessage.role === 'assistant' &&
        lastMessage.status.type === 'complete' &&
        lastMessage.content.toLowerCase().includes('provide feedback')) ||
      lastMessage?.content.toLowerCase().includes('please confirm')
    ) {
      sendFeedbackNotification();
    }
  }, [
    isFeedbackPage,
    currentStep,
    sendFeedbackNotification,
    conversationData?.pages,
  ]);

  // Reset notification sent flag when step changes
  useEffect(() => {
    if (
      currentStep === ToolCreationState.CREATING_CODE ||
      currentStep === ToolCreationState.CREATING_METADATA
    ) {
      void debug(`resetting notification sent flag, new state ${currentStep}`);
      notificationSentRef.current = false;
    }
  }, [currentStep]);

  useEffect(() => {
    feedbackNotificationSentRef.current = false;
  }, [location.pathname]);

  useEffect(() => {
    if (!toolCode || !forceGenerateMetadata.current || !chatInboxId) return;
    const generateMetadata = async () => {
      const currentTools = tools || form.watch('tools');

      await createToolMetadata({
        nodeAddress: auth?.node_address ?? '',
        token: auth?.api_v2_key ?? '',
        jobId: extractJobIdFromInbox(chatInboxId),
        tools: currentTools,
      });
    };

    void generateMetadata();
  }, [
    toolCode,
    chatInboxId,
    auth?.node_address,
    auth?.api_v2_key,
    createToolMetadata,
    form,
    tools,
    setCurrentStep,
    forceGenerateMetadata,
  ]);

  const startToolCreation = async (data: CreateToolCodeFormSchema) => {
    if (!auth) return;
    if (!data.llmProviderId) {
      toast.error('Please select an AI model');
      return;
    }
    setToolCreationError(null);

    if (requireFeedbackFlow) setCurrentStep(ToolCreationState.PLAN_REVIEW);
    else setCurrentStep(ToolCreationState.CREATING_CODE);

    await createToolCode({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      message: data.message,
      llmProviderId: data.llmProviderId,
      jobId: chatInboxId ? extractJobIdFromInbox(chatInboxId) : undefined,
      tools: data.tools,
      language: data.language,
      thinking: data.thinking,
    });

    form.setValue('message', '');
  };

  const handleContinueConversation = async (message: string) => {
    if (!chatInboxId || !auth) return;

    setCurrentStep(ToolCreationState.CREATING_CODE);
    await createToolCode({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      message,
      llmProviderId: form.watch('llmProviderId'),
      jobId: extractJobIdFromInbox(chatInboxId),
      tools: form.watch('tools'),
      language: form.watch('language'),
      thinking: form.watch('thinking'),
    });

    form.setValue('message', '');
  };

  const generateMetadata = async () => {
    if (!auth || !chatInboxId) return;

    try {
      const currentTools = tools || form.watch('tools');

      await createToolMetadata({
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        jobId: extractJobIdFromInbox(chatInboxId),
        tools: currentTools,
        xShinkaiToolId: xShinkaiToolId, // Add tool ID to help server identify the context
      });
    } catch (error) {
      console.error('Error generating metadata:', error);
      setToolMetadataError(
        `Failed to regenerate metadata: ${error instanceof Error ? error.message : 'Unknown error'}`,
      );
    }
  };

  useEffect(() => {
    if (
      !isPlaygroundMode &&
      toolCode &&
      toolMetadata &&
      toolCodeStatus === 'success' &&
      toolMetadataStatus === 'success' &&
      forceAutoSave.current &&
      !isSavingTool
    ) {
      forceAutoSave.current = false;
      forceGenerateMetadata.current = false;
      forceGenerateCode.current = false;
      void handleSaveTool({
        toolMetadata: toolMetadata,
        toolCode: toolCode,
        tools: form.getValues('tools'),
        language: form.getValues('language'),
        shouldPrefetchPlaygroundTool: true,
        onSuccess: () => {
          forceAutoSave.current = false;
          forceGenerateMetadata.current = false;
          forceGenerateCode.current = false;
          setCurrentStep(ToolCreationState.COMPLETED);
          captureAnalyticEvent('Custom Tool Created', undefined);
        },
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    toolCode,
    toolMetadata,
    toolCodeStatus,
    toolMetadataStatus,
    currentStep,
    isPlaygroundMode,
    handleSaveTool,
    form,
    setCurrentStep,
    isSavingTool,
  ]);

  const handleApplyChangesCodeSubmit = useCallback(
    async (e: React.FormEvent<HTMLFormElement>) => {
      e.preventDefault();
      if (!chatInboxId || !auth) return;

      const data = new FormData(e.currentTarget);
      const currentEditorValue = data.get('editor');
      setResetCounter((prev) => prev + 1);
      setIsDirtyCodeEditor(false);
      baseToolCodeRef.current = currentEditorValue as string;

      await updateToolCodeImplementation({
        token: auth.api_v2_key,
        nodeAddress: auth.node_address,
        jobId: extractJobIdFromInbox(chatInboxId),
        code: currentEditorValue as string,
      });

      setTimeout(() => {
        setToolCode(currentEditorValue as string);
      }, 100);
    },
    [
      chatInboxId,
      auth,
      setResetCounter,
      setIsDirtyCodeEditor,
      updateToolCodeImplementation,
      setToolCode,
    ],
  );

  const resetToolCode = useCallback(() => {
    setIsDirtyCodeEditor(false);
    setResetCounter((prev) => prev + 1);
    forceGenerateMetadata.current = true;
    setTimeout(() => {
      setToolCode(baseToolCodeRef.current);
    }, 0);
  }, [setIsDirtyCodeEditor, setResetCounter, setToolCode]);

  const executeToolCode: FormProps['onSubmit'] = useCallback(
    async (data: any) => {
      if (!auth) return;
      mountTimestamp.current = new Date();
      const { configs, params } = data.formData;

      const selectedFilePathsInParams = Object.keys(params || {}).filter(
        (key) => key.toLowerCase().includes('file_path'),
      );

      const currentCode = codeEditorRef?.current?.value ?? toolCode ?? '';

      await executeToolCodeQuery.mutateAsync({
        code: currentCode,
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        params,
        llmProviderId: form.getValues('llmProviderId'),
        tools: form.getValues('tools'),
        language: form.getValues('language'),
        configs,
        xShinkaiAppId,
        xShinkaiToolId,
        ...(selectedFilePathsInParams.length > 0 && {
          mounts: selectedFilePathsInParams.map((key) => params[key]),
        }),
      });
    },
    [executeToolCodeQuery, form, auth, toolCode, xShinkaiAppId, xShinkaiToolId],
  );

  const toolResultFiles = useMemo(() => {
    return (toolResult as any)?.__created_files__ ?? [];
  }, [toolResult]);

  const toolResultWithoutFiles = useMemo(() => {
    if (!toolResult) {
      return {};
    }

    const { __created_files__, ...rest } = toolResult as any;
    return rest;
  }, [toolResult]);

  return {
    currentStep,
    error: toolCreationError,

    mountTimestamp,

    toolCode,
    toolCodeStatus,
    toolCodeError,
    baseToolCodeRef,

    toolMetadata,
    toolMetadataStatus,
    toolMetadataError,

    isDirtyCodeEditor,
    setIsDirtyCodeEditor,
    resetCounter,
    codeEditorRef,
    metadataEditorRef,

    chatConversationData: formattedConversationData,
    fetchPreviousPage,
    hasPreviousPage,
    isChatConversationLoading,
    isFetchingPreviousPage,
    isChatConversationSuccess,
    toolHistory,

    executeToolCode,
    executeToolCodeQuery,
    toolResultFiles,
    toolResult: toolResultWithoutFiles,

    isCreatingToolCode,
    isCreatingMetadata,
    isSavingTool,

    startToolCreation,
    generateMetadata,
    handleContinueConversation,
    handleApplyChangesCodeSubmit,
    resetToolCode,
    forceGenerateMetadata,
  };
};
