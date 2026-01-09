import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { useUpdateAgentInJob } from '@shinkai_network/shinkai-node-state/v2/mutations/updateAgentInJob/useUpdateAgentInJob';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { useGetProviderFromJob } from '@shinkai_network/shinkai-node-state/v2/queries/getProviderFromJob/useGetProviderFromJob';
import {
  Button,
  ChatInputArea,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
  DialogTrigger,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  Skeleton,
} from '@shinkai_network/shinkai-ui';
import { SendIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { AnimatePresence, motion } from 'framer-motion';
import { LoaderIcon, LogOut } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router';

import { MessageList } from '../components/chat/components/message-list';
import { getRandomWidth } from '../components/playground-tool/components/code-panel';
import {
  ToolCreationState,
  usePlaygroundStore,
} from '../components/playground-tool/context/playground-context';
import { useToolForm } from '../components/playground-tool/hooks/use-tool-code';
import { useToolFlow } from '../components/playground-tool/hooks/use-tool-flow';
import PlaygroundToolLayout from '../components/playground-tool/layout';
import {
  CODE_GENERATOR_MODEL_ID,
  SHINKAI_FREE_TRIAL_MODEL_ID,
} from '../components/tools/constants';
import { useAuth } from '../store/auth';
import { useSettings } from '../store/settings';

function ToolFeedbackPrompt() {
  const { inboxId } = useParams();
  const { state } = useLocation();
  const form = useToolForm({ ...state?.form, message: '' });
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [isExitDialogOpen, setIsExitDialogOpen] = useState(false);
  const auth = useAuth((state) => state.auth);
  const defaultAgentId = useSettings((state) => state.defaultAgentId);

  const jobId = inboxId ? extractJobIdFromInbox(inboxId) : undefined;

  // Get current provider for this job
  const { data: currentProvider } = useGetProviderFromJob({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    jobId: jobId ?? '',
  });

  // Get all LLM providers to find the free trial model
  const { data: llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  // Update agent in job mutation
  const { mutateAsync: updateAgentInJob } = useUpdateAgentInJob({
    onSuccess: () => {},
    onError: (error) => {
      console.error('Failed to switch LLM provider:', error);
    },
  });

  const {
    currentStep,
    isCreatingToolCode,
    startToolCreation,
    chatConversationData: conversationData,
    fetchPreviousPage,
    hasPreviousPage,
    isChatConversationLoading,
    isFetchingPreviousPage,
    isChatConversationSuccess,
    error,
  } = useToolFlow({
    form,
    initialInboxId: inboxId,
  });

  const isLoadingMessage = useMemo(() => {
    const lastMessage = conversationData?.pages?.at(-1)?.at(-1);
    return (
      !!inboxId &&
      lastMessage &&
      lastMessage.role === 'assistant' &&
      lastMessage.status.type === 'running'
    );
  }, [conversationData?.pages, inboxId]);

  const resetPlaygroundStore = usePlaygroundStore(
    (state) => state.resetPlaygroundStore,
  );

  useEffect(() => {
    if (error) {
      resetPlaygroundStore();
      void navigate('/tools');
    }
  }, [error, resetPlaygroundStore, navigate]);

  // Effect to switch from CODE_GENERATOR to SHINKAI_FREE_TRIAL model when tool creation is completed
  useEffect(() => {
    if (!auth || !currentProvider || !llmProviders || !jobId) return;

    // Only switch models when the tool creation is completed
    if (currentStep !== ToolCreationState.COMPLETED) return;

    const isCodeGeneratorModel = currentProvider.agent.model?.toLowerCase() === CODE_GENERATOR_MODEL_ID.toLowerCase();

    if (isCodeGeneratorModel) {
      const freeTrialProvider = llmProviders.find(
        (provider) => provider.model.toLowerCase() === SHINKAI_FREE_TRIAL_MODEL_ID.toLowerCase()
      );

      // Use free trial provider if available, otherwise fall back to default provider
      const targetProvider = freeTrialProvider || llmProviders.find(
        (provider) => provider.id === defaultAgentId
      );

      if (targetProvider && targetProvider.id !== currentProvider.agent.id) {
        // Switch the LLM provider for the job after tool creation is complete
        void updateAgentInJob({
          nodeAddress: auth.node_address,
          token: auth.api_v2_key,
          jobId,
          newAgentId: targetProvider.id,
        });

        // Update the form to reflect the new provider
        form.setValue('llmProviderId', targetProvider.id);
      }
    }
  }, [auth, currentProvider, llmProviders, jobId, currentStep, updateAgentInJob, form, defaultAgentId]);

  const renderStep = useCallback(() => {
    switch (currentStep) {
      case ToolCreationState.PROMPT_INPUT:
      case ToolCreationState.PLAN_REVIEW:
        return (
          <div
            className={cn(
              'flex size-full flex-col items-center justify-center py-4',
            )}
          >
            <motion.div
              className={cn(
                'mx-auto flex h-full w-full flex-col items-stretch justify-center rounded-xl p-1 pt-0 pb-3',
              )}
              layoutId={`left-element`}
            >
              <div className="flex items-center justify-between px-4">
                <Dialog
                  onOpenChange={setIsExitDialogOpen}
                  open={isExitDialogOpen}
                >
                  <DialogTrigger asChild>
                    <Button
                      className="size-6 p-1"
                      onClick={() => setIsExitDialogOpen(true)}
                      size="auto"
                      variant="tertiary"
                    >
                      <LogOut className="size-full text-white" />
                    </Button>
                  </DialogTrigger>
                  <DialogContent className="sm:max-w-[425px]">
                    <DialogTitle className="pb-0">
                      Exit Tool Creation
                    </DialogTitle>
                    <DialogDescription>
                      Are you sure you want to exit? Your progress will be lost
                      and you won’t be able to return to this session.
                    </DialogDescription>
                    <DialogFooter>
                      <div className="flex gap-2 pt-4">
                        <Button
                          className="min-w-[100px] flex-1"
                          onClick={() => setIsExitDialogOpen(false)}
                          size="sm"
                          type="button"
                          variant="outline"
                        >
                          Cancel
                        </Button>
                        <Button
                          className="min-w-[100px] flex-1"
                          onClick={() => {
                            resetPlaygroundStore();
                            void navigate('/tools');
                            setIsExitDialogOpen(false);
                          }}
                          size="sm"
                        >
                          Exit
                        </Button>
                      </div>
                    </DialogFooter>
                  </DialogContent>
                </Dialog>
                <h1 className="font-clash py-3 text-center font-semibold">
                  Creating Tool
                </h1>
                <div className="size-6" />
              </div>
              {isChatConversationLoading ? (
                <div className="bg-bg-dark flex w-full flex-1 flex-col gap-4 overflow-y-auto p-4">
                  <Skeleton className="h-6 w-32" />
                  <Skeleton className="h-24 w-full" />
                  <Skeleton className="h-24 w-full" />
                  <Skeleton className="h-6 w-32" />
                  <Skeleton className="h-24 w-full" />
                  <Skeleton className="h-24 w-full" />
                  <Skeleton className="h-6 w-32" />
                  <Skeleton className="h-24 w-full" />
                </div>
              ) : (
                <MessageList
                  containerClassName="px-3 pt-2 flex-1"
                  disabledRetryAndEdit={true}
                  fetchPreviousPage={fetchPreviousPage}
                  hasPreviousPage={hasPreviousPage}
                  hidePythonExecution={true}
                  inboxId={inboxId ?? ''}
                  isFetchingPreviousPage={isFetchingPreviousPage}
                  isLoading={isChatConversationLoading}
                  isSuccess={isChatConversationSuccess}
                  minimalistMode
                  noMoreMessageLabel={t('chat.allMessagesLoaded')}
                  paginatedMessages={conversationData}
                />
              )}
              <Form {...form}>
                <form
                  className="relative shrink-0 space-y-2 px-3 pt-6"
                  onSubmit={form.handleSubmit(startToolCreation)}
                >
                  <div className="flex shrink-0 items-center gap-1">
                    <FormField
                      control={form.control}
                      name="message"
                      render={({ field }) => (
                        <FormItem className="flex-1 space-y-0">
                          <FormLabel className="sr-only">
                            {t('chat.enterMessage')}
                          </FormLabel>
                          <FormControl>
                            <div className="space-y-1.5">
                              <AnimatePresence>
                                {isLoadingMessage && (
                                  <motion.div
                                    animate={{ opacity: 1, y: 0 }}
                                    className="absolute inset-x-2 -top-[24px] flex items-center justify-start gap-2 rounded-t-lg bg-cyan-900/20 px-2 py-1 text-xs text-cyan-500"
                                    exit={{ opacity: 0, y: 10 }}
                                    initial={{ opacity: 0, y: 10 }}
                                    transition={{ duration: 0.2 }}
                                  >
                                    <LoaderIcon className="size-4 animate-spin" />
                                    Thinking...
                                  </motion.div>
                                )}
                              </AnimatePresence>
                              <ChatInputArea
                                autoFocus
                                bottomAddons={
                                  <div className="relative z-50 flex items-end gap-3 self-end p-2">
                                    <span className="text-text-secondary pb-1 text-xs font-light">
                                      <span className="font-medium">Enter</span>{' '}
                                      to send
                                    </span>

                                    <Button
                                      className={cn('size-[36px] p-2')}
                                      disabled={
                                        !form.watch('message') ||
                                        isCreatingToolCode ||
                                        isLoadingMessage
                                      }
                                      isLoading={isCreatingToolCode}
                                      size="icon"
                                      type="submit"
                                    >
                                      <SendIcon className="h-full w-full" />
                                      <span className="sr-only">
                                        {t('chat.sendMessage')}
                                      </span>
                                    </Button>
                                  </div>
                                }
                                disabled={
                                  isCreatingToolCode || isLoadingMessage
                                }
                                onChange={field.onChange}
                                onSubmit={() => {
                                  void startToolCreation(form.getValues());
                                }}
                                placeholder="Send message..."
                                value={field.value}
                              />
                            </div>
                          </FormControl>
                        </FormItem>
                      )}
                    />
                  </div>
                </form>
              </Form>
            </motion.div>
          </div>
        );
      case ToolCreationState.CREATING_CODE:
      case ToolCreationState.CREATING_METADATA:
      case ToolCreationState.SAVING_TOOL:
      case ToolCreationState.COMPLETED:
        return (
          <div className={cn('size-full')}>
            <PlaygroundToolLayout
              leftElement={
                <motion.div
                  className={cn('flex flex-1 flex-col overflow-y-auto px-2')}
                  layoutId={`left-element`}
                >
                  <MessageList
                    containerClassName="px-3 pt-2"
                    disabledRetryAndEdit={true}
                    fetchPreviousPage={fetchPreviousPage}
                    hasPreviousPage={hasPreviousPage}
                    hidePythonExecution={true}
                    inboxId={inboxId ?? ''}
                    isFetchingPreviousPage={isFetchingPreviousPage}
                    isLoading={isChatConversationLoading}
                    isSuccess={isChatConversationSuccess}
                    minimalistMode
                    noMoreMessageLabel={t('chat.allMessagesLoaded')}
                    paginatedMessages={conversationData}
                  />

                  <Form {...form}>
                    <form
                      className="shrink-0 space-y-2 px-3 pt-2"
                      onSubmit={form.handleSubmit(startToolCreation)}
                    >
                      <div className="flex shrink-0 items-center gap-1">
                        <FormField
                          control={form.control}
                          name="message"
                          render={({ field }) => (
                            <FormItem className="flex-1 space-y-0">
                              <FormLabel className="sr-only">
                                {t('chat.enterMessage')}
                              </FormLabel>
                              <FormControl>
                                <div className="space-y-1.5">
                                  <ChatInputArea
                                    autoFocus
                                    bottomAddons={
                                      <div className="relative z-50 flex items-end gap-3 self-end p-2">
                                        <span className="text-text-secondary pb-1 text-xs font-light">
                                          <span className="font-medium">
                                            Enter
                                          </span>{' '}
                                          to send
                                        </span>

                                        <Button
                                          className={cn('size-[36px] p-2')}
                                          size="icon"
                                          type="submit"
                                        >
                                          <SendIcon className="h-full w-full" />
                                          <span className="sr-only">
                                            {t('chat.sendMessage')}
                                          </span>
                                        </Button>
                                      </div>
                                    }
                                    disabled
                                    onChange={field.onChange}
                                    onSubmit={() => {
                                      void startToolCreation(form.getValues());
                                    }}
                                    placeholder="Send message..."
                                    value={field.value}
                                  />
                                </div>
                              </FormControl>
                            </FormItem>
                          )}
                        />
                      </div>
                    </form>
                  </Form>
                </motion.div>
              }
              rightElement={
                <div className="flex size-full flex-col items-center justify-center gap-1 p-1 text-xs">
                  <div className="relative mx-auto h-[400px] w-full max-w-2xl overflow-hidden rounded-lg md:order-2 md:h-[450px] lg:h-[600px]">
                    <motion.div
                      animate={{ y: 0, opacity: 1 }}
                      className="border-divider bg-bg-dark flex h-full w-full flex-1 flex-col gap-3 overflow-hidden rounded-lg p-3"
                      exit={{ y: -100, opacity: 0 }}
                      initial={{ y: 100, opacity: 0, rotateX: -20 }}
                      key={currentStep}
                      transition={{ duration: 0.5, ease: 'easeInOut' }}
                    >
                      <div className="flex items-center gap-3">
                        {currentStep === ToolCreationState.CREATING_CODE ? (
                          <LoaderIcon className="size-4 animate-spin text-cyan-500" />
                        ) : currentStep ===
                          ToolCreationState.CREATING_METADATA ? (
                          <LoaderIcon className="size-4 animate-spin text-cyan-500" />
                        ) : currentStep === ToolCreationState.SAVING_TOOL ? (
                          <LoaderIcon className="size-4 animate-spin text-cyan-500" />
                        ) : currentStep === ToolCreationState.COMPLETED ? (
                          <div className="flex h-5 w-5 items-center justify-center rounded-full bg-cyan-500/20 text-cyan-500">
                            ✓
                          </div>
                        ) : null}
                        {/* : currentStep === 'error' ? (
                          <div className="flex h-5 w-5 items-center justify-center rounded-full bg-red-500/20 text-red-500">
                            ✗
                          </div>
                        )  */}
                        <h3 className="font-medium text-zinc-100">
                          {currentStep === ToolCreationState.CREATING_CODE
                            ? 'Generating Code...'
                            : currentStep ===
                                ToolCreationState.CREATING_METADATA
                              ? 'Generating Metadata...'
                              : currentStep === ToolCreationState.SAVING_TOOL
                                ? 'Saving Code & Preview...'
                                : currentStep === ToolCreationState.COMPLETED
                                  ? 'Code & Preview Saved'
                                  : null}
                        </h3>
                      </div>

                      <motion.div
                        animate={{ opacity: 1 }}
                        className="size-w flex flex-1 flex-col items-start gap-1 overflow-hidden rounded-md px-4 py-4 text-xs"
                        exit={{ opacity: 0 }}
                        initial={{ opacity: 0 }}
                        transition={{
                          duration: 0.5,
                          ease: 'easeInOut',
                        }}
                      >
                        {[...Array(20)].map((_, lineIndex) => (
                          <div className="mb-2 flex gap-3" key={lineIndex}>
                            <Skeleton className="h-4 w-12 rounded" />
                            <div className="flex-1">
                              <div className="flex flex-wrap gap-2">
                                {[
                                  ...Array(Math.floor(Math.random() * 4) + 1),
                                ].map((_, blockIndex) => (
                                  <Skeleton
                                    className={cn(
                                      getRandomWidth(),
                                      'h-4 rounded',
                                    )}
                                    key={blockIndex}
                                  />
                                ))}
                              </div>
                            </div>
                          </div>
                        ))}
                      </motion.div>
                    </motion.div>
                  </div>
                </div>
              }
              topElement={
                <div className="flex h-[45px] items-center justify-between gap-2 border-b border-gray-400 px-4 pb-2.5">
                  <Skeleton className="h-[30px] w-[200px]" />
                  <div className="flex items-center gap-2">
                    <Skeleton className="h-[30px] w-[100px]" />
                    <Skeleton className="h-[30px] w-[40px]" />
                    <Skeleton className="h-[30px] w-[100px]" />
                  </div>
                </div>
              }
            />
          </div>
        );
      // case 'error':
      //   return (
      //     <div className="flex h-full items-center justify-center">
      //       <div className="text-center text-sm text-text-secondary">
      //         <h1>Error</h1>
      //         <p>{error}</p>
      //       </div>
      //     </div>
      //   );
      default:
        return null;
    }
  }, [
    currentStep,
    isChatConversationLoading,
    fetchPreviousPage,
    hasPreviousPage,
    isFetchingPreviousPage,
    isChatConversationSuccess,
    t,
    conversationData,
    form,
    navigate,
    startToolCreation,
    isCreatingToolCode,
    isExitDialogOpen,
    isLoadingMessage,
    resetPlaygroundStore,
    setIsExitDialogOpen,
  ]);

  return <div className={cn('h-full overflow-auto')}>{renderStep()}</div>;
}

export default ToolFeedbackPrompt;
