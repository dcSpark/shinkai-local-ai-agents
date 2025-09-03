import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type ToolArgs,
  ToolStatusType,
} from '@shinkai_network/shinkai-message-ts/api/general/types';

import {
  type AssistantMessage,
  type FormattedMessage,
  type TextStatus,
  type ToolCall,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  Button,
  ChatInputArea,
  CopyToClipboardIcon,
  DotsLoader,
  FileList,
  Form,
  FormField,
  MarkdownText,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
  PrettyJsonPrint,
} from '@shinkai_network/shinkai-ui';
import {
  AIAgentIcon,
  ReasoningIcon,
  ToolsIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';

import equal from 'fast-deep-equal';
import { AnimatePresence, motion } from 'framer-motion';
import {
  BotIcon,
  Edit3,
  InfoIcon,
  Loader2,
  RotateCcw,
  Unplug,
  XCircle,
} from 'lucide-react';
import React, { Fragment, memo, useEffect, useMemo, useState } from 'react';
import { useForm } from 'react-hook-form';
import { Link } from 'react-router';
import { z } from 'zod';
import ProviderIcon from '../../../components/ais/provider-icon';

import { useOAuth } from '../../../store/oauth';
import { useSettings } from '../../../store/settings';
import { oauthUrlMatcherFromErrorMessage } from '../../../utils/oauth';

export const extractErrorPropertyOrContent = (
  content: string,
  property: 'error' | 'error_message',
) => {
  try {
    const parsedContent = JSON.parse(content);
    if (property in parsedContent) {
      return parsedContent[property];
    }
  } catch {
    /* ignore */
  }
  return String(content);
};

type MessageProps = {
  messageId: string;
  isPending?: boolean;
  message: FormattedMessage;
  handleRetryMessage?: () => void;

  disabledRetry?: boolean;
  disabledEdit?: boolean;
  handleEditMessage?: (message: string) => void;
  messageExtra?: React.ReactNode;
};

export const editMessageFormSchema = z.object({
  message: z.string().min(1),
});

type EditMessageFormSchema = z.infer<typeof editMessageFormSchema>;

export const MessageBase = ({
  message,
  // messageId
  isPending,
  handleRetryMessage,

  disabledRetry,
  disabledEdit,
  handleEditMessage,
}: MessageProps) => {
  const { t } = useTranslation();

  const getChatFontSizeInPts = useSettings(
    (state) => state.getChatFontSizeInPts,
  );

  const [editing, setEditing] = useState(false);

  const editMessageForm = useForm<EditMessageFormSchema>({
    resolver: zodResolver(editMessageFormSchema),
    defaultValues: {
      message: message.content,
    },
  });

  const { message: currentMessage } = editMessageForm.watch();

  const onSubmit = async (data: z.infer<typeof editMessageFormSchema>) => {
    if (message.role === 'user') {
      handleEditMessage?.(data.message);
      setEditing(false);
    }
  };

  useEffect(() => {
    if (message.role === 'user') {
      editMessageForm.reset({ message: message.content });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editMessageForm, message.content]);

  const oauthUrl = useMemo(() => {
    return oauthUrlMatcherFromErrorMessage(message.content);
  }, [message.content]);

  const configDeepLinkMatcher = (content: string) => {
    const match = content.match(/shinkai:\/\/config\?tool=([^\s]+)/);
    return match ? match[1] : null;
  };

  const configDeepLinkToolRouterKey = useMemo(() => {
    if (!message?.content) return null;
    return configDeepLinkMatcher(message.content);
  }, [message]);

  const { setOauthModalVisible } = useOAuth();

  const selectedIcon = useMemo(() => {
    if (message.role !== 'assistant') return null;
    if (message.provider?.provider_type === 'LLMProvider') {
      return (
        <ProviderIcon
          className="mx-1 size-4"
          provider={message.provider?.agent.model.split(':')[0]}
        />
      );
    }
    if (message.provider?.provider_type === 'Agent') {
      return <AIAgentIcon name={message.provider?.agent.id} size={'xs'} />;
    }
    return <BotIcon className="mr-1 size-4" />;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    (message as AssistantMessage)?.provider?.agent.id,
    (message as AssistantMessage)?.provider?.agent.model,
    (message as AssistantMessage)?.provider?.provider_type,
    message.role,
  ]);

  return (
    <motion.div
      animate="rest"
      className={cn('pb-3')}
      data-testid={`message-${
        message.role === 'user' ? 'local' : 'remote'
      }-${message.messageId}`}
      id={message.messageId}
      initial="rest"
      style={{ fontSize: `${getChatFontSizeInPts()}px` }}
      // whileHover="hover"
    >
      <div
        className={cn(
          'relative flex flex-row space-x-2',
          message.role === 'user' &&
            'mr-0 ml-auto flex-row-reverse space-x-reverse',
          message.role === 'assistant' && 'mr-auto ml-0 flex-row items-start',
        )}
      >
        {message.role === 'assistant' ? (
          <div className="mt-2">{selectedIcon}</div>
        ) : null}
        <div
          className={cn(
            'text-em-base flex flex-col overflow-hidden bg-transparent text-white',
            editing && 'w-full py-1',
          )}
        >
          {editing ? (
            <Form {...editMessageForm}>
              <form
                className="relative flex items-center"
                onSubmit={editMessageForm.handleSubmit(onSubmit)}
              >
                <div className="w-full">
                  <FormField
                    control={editMessageForm.control}
                    name="message"
                    render={({ field }) => (
                      <ChatInputArea
                        bottomAddons={
                          <div className="flex w-full items-center justify-between px-1">
                            <div className="text-em-xs text-text-secondary flex items-center gap-1">
                              <InfoIcon className="text-text-secondary h-3 w-3" />
                              <span>{t('chat.editMessage.warning')}</span>
                            </div>
                            <div className="flex items-center gap-2">
                              <Button
                                className="min-w-[90px]"
                                onClick={() => setEditing(false)}
                                rounded="lg"
                                size="xs"
                                variant="outline"
                              >
                                {t('common.cancel')}
                              </Button>
                              <Button
                                className="min-w-[90px]"
                                disabled={!currentMessage}
                                onClick={editMessageForm.handleSubmit(onSubmit)}
                                rounded="lg"
                                size="xs"
                              >
                                {t('common.send')}
                              </Button>
                            </div>
                          </div>
                        }
                        onChange={field.onChange}
                        onSubmit={editMessageForm.handleSubmit(onSubmit)}
                        value={field.value}
                      />
                    )}
                  />
                </div>
              </form>
            </Form>
          ) : (
            <Fragment>
              <div
                className={cn(
                  'relative mt-1 flex flex-col rounded-lg px-3.5 pt-3 text-white',
                  message.role === 'user'
                    ? 'bg-bg-secondary rounded-tr-none'
                    : '',
                  !message.content ? 'pb-3' : 'pb-4',
                  editing && 'w-full py-1',
                  message.role === 'assistant' &&
                    isPending &&
                    'relative overflow-hidden pb-4 before:absolute before:right-0 before:bottom-0 before:left-0 before:h-10 before:animate-pulse before:bg-gradient-to-l before:from-gray-200 before:to-gray-200/10',
                  'rounded-lg px-2 pt-1.5 pb-1.5',
                  message.role === 'assistant' && 'p-0',
                )}
              >
                {message.role === 'assistant' ? (
                  <div className="text-em-base mt-0.5 mb-3 font-bold text-white">
                    {formatText(message.provider?.agent.name ?? '')}
                  </div>
                ) : null}
                {message.role === 'assistant' && message.reasoning != null && (
                  <Reasoning
                    reasoning={message.reasoning.text}
                    status={message.reasoning.status}
                  />
                )}

                {message.role === 'assistant' &&
                  message.toolCalls &&
                  message.toolCalls.length > 0 && (
                    <Accordion
                      className="max-w-full space-y-1.5 self-baseline overflow-x-auto pb-3"
                      type="multiple"
                    >
                      {message.toolCalls.map((tool, index) => {
                        return (
                          <AccordionItem
                            className="bg-bg-dark border-divider overflow-hidden rounded-lg border"
                            disabled={tool.status !== ToolStatusType.Complete}
                            key={`${tool.name}-${index}`}
                            value={`${tool.name}-${index}`}
                          >
                            <AccordionTrigger
                              className={cn(
                                'min-w-[10rem] gap-3 py-0 pr-2 no-underline hover:no-underline',
                                'hover:bg-bg-default [&[data-state=open]]:bg-bg-default transition-colors',
                                tool.status !== ToolStatusType.Complete &&
                                  '[&>svg]:hidden',
                              )}
                            >
                              <ToolCard
                                args={tool.args}
                                name={tool.name}
                                status={tool.status ?? ToolStatusType.Complete}
                                toolRouterKey={tool.toolRouterKey}
                              />
                            </AccordionTrigger>
                            <AccordionContent className="bg-bg-dark flex flex-col gap-1 rounded-b-lg px-3 pt-2 pb-3 text-xs">
                              {Object.keys(tool.args).length > 0 && (
                                <span className="font-medium text-white">
                                  {tool.name}(
                                  {Object.keys(tool.args).length > 0 && (
                                    <span className="text-text-secondary font-mono font-medium">
                                      <PrettyJsonPrint json={tool.args} />
                                    </span>
                                  )}
                                  )
                                </span>
                              )}
                              {tool.result && (
                                <div>
                                  <span>Response:</span>
                                  <span className="text-text-secondary font-mono break-all">
                                    <PrettyJsonPrint json={tool.result} />
                                  </span>
                                </div>
                              )}
                            </AccordionContent>
                          </AccordionItem>
                        );
                      })}
                    </Accordion>
                  )}

                {message.role === 'assistant' && (
                  <MarkdownText
                    className={cn(
                      message.reasoning?.status?.type === 'running' &&
                        'text-text-secondary',
                    )}
                    content={extractErrorPropertyOrContent(
                      message.content,
                      'error_message',
                    )}
                    isRunning={
                      !!message.content && message.status.type === 'running'
                    }
                  />
                )}

                {message.role === 'user' && (
                  <div className="whitespace-pre-line">{message.content}</div>
                )}
                {message.role === 'assistant' &&
                  message.toolCalls?.some(
                    (tool) => tool.status === 'Running',
                  ) &&
                  message.content === '' && (
                    <div className="pt-1.5 whitespace-pre-line">
                      <span className="text-text-secondary text-xs">
                        Executing tools
                      </span>
                    </div>
                  )}
                {message.role === 'assistant' &&
                  message.toolCalls?.length > 0 &&
                  message.toolCalls?.every(
                    (tool) => tool.status === 'Complete',
                  ) &&
                  message.content === '' && (
                    <div className="pt-1.5 whitespace-pre-line">
                      <span className="text-text-secondary text-xs">
                        Getting AI response
                      </span>
                    </div>
                  )}
                {message.role === 'assistant' &&
                  message.status.type === 'running' &&
                  message.content === '' && (
                    <div className="pt-1.5 whitespace-pre-line">
                      <DotsLoader />
                    </div>
                  )}

                {oauthUrl && (
                  <div className="bg-bg-tertiary mt-4 flex flex-col items-start rounded-lg p-4">
                    <p className="text-em-lg mb-2 font-semibold text-white">
                      <div className="flex items-center">
                        <Unplug className="mr-2 h-5 w-5" />
                        {t('oauth.connectionRequired')}
                      </div>
                    </p>
                    <p className="text-em-sm mb-4 text-white">
                      {t('oauth.connectionRequiredDescription', {
                        provider: new URL(oauthUrl).hostname,
                      })}
                    </p>
                    <Button
                      className="rounded-lg px-4 py-2 text-white transition duration-300"
                      onClick={() =>
                        setOauthModalVisible({ visible: true, url: oauthUrl })
                      }
                      variant="outline"
                    >
                      {t('oauth.connectNow')}
                    </Button>
                  </div>
                )}

                {configDeepLinkToolRouterKey && (
                  <div className="mt-4 flex flex-col items-start rounded-lg bg-gray-950 p-6 shadow-lg">
                    <p className="text-em-base mb-3 font-semibold text-white">
                      <div className="flex items-center">
                        <ToolsIcon className="text-brand mr-3 size-4" />
                        {t('tools.setupRequired')}
                      </div>
                    </p>
                    <p className="text-em-base text-text-secondary mb-5 leading-relaxed">
                      {t('tools.setupDescription')}
                    </p>
                    <Link to={`/tools/${configDeepLinkToolRouterKey}`}>
                      <Button variant="default" size="sm">
                        <ToolsIcon className="h-4 w-4 transition-transform group-hover:rotate-12" />
                        {t('tools.setupNow')}
                      </Button>
                    </Link>
                  </div>
                )}

                {message.role === 'user' && !!message.attachments.length && (
                  <FileList className="mt-2" files={message.attachments} />
                )}

                {message.role === 'assistant' &&
                  message.toolCalls?.some((tool) => !!tool.generatedFiles) && (
                    <GeneratedFiles toolCalls={message.toolCalls} />
                  )}
              </div>

              <div
                className={cn(
                  'flex items-center justify-start gap-3 py-3',
                  message.role === 'user' && 'flex-row-reverse',
                  message.role === 'assistant' &&
                    message.status.type === 'running' &&
                    'hidden',
                )}
              >
                <div className="flex items-center gap-1.5">
                  {message.role === 'user' && !disabledEdit && (
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button
                          className={cn(
                            'text-text-secondary border-divider hover:bg-bg-tertiary flex h-7 w-7 items-center justify-center rounded-lg border bg-transparent transition-colors hover:text-white [&>svg]:h-3 [&>svg]:w-3',
                          )}
                          onClick={() => {
                            setEditing(true);
                          }}
                        >
                          <Edit3 />
                        </button>
                      </TooltipTrigger>
                      <TooltipPortal>
                        <TooltipContent>
                          <p>{t('common.editMessage')}</p>
                        </TooltipContent>
                      </TooltipPortal>
                    </Tooltip>
                  )}
                  {message.role === 'assistant' && !disabledRetry && (
                    <>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <button
                            className={cn(
                              'text-text-secondary border-divider hover:bg-bg-tertiary flex h-7 w-7 items-center justify-center rounded-lg border bg-transparent transition-colors hover:text-white [&>svg]:h-3 [&>svg]:w-3',
                            )}
                            onClick={handleRetryMessage}
                          >
                            <RotateCcw />
                          </button>
                        </TooltipTrigger>
                        <TooltipPortal>
                          <TooltipContent>
                            <p>{t('common.retry')}</p>
                          </TooltipContent>
                        </TooltipPortal>
                      </Tooltip>
                    </>
                  )}

                  <Tooltip>
                    <TooltipTrigger asChild>
                      <div>
                        <CopyToClipboardIcon
                          className={cn(
                            'text-text-secondary border-divider hover:bg-bg-tertiary h-7 w-7 border bg-transparent [&>svg]:h-3 [&>svg]:w-3',
                          )}
                          string={extractErrorPropertyOrContent(
                            message.content,
                            'error_message',
                          )}
                        />
                      </div>
                    </TooltipTrigger>
                    <TooltipPortal>
                      <TooltipContent>
                        <p>{t('common.copy')}</p>
                      </TooltipContent>
                    </TooltipPortal>
                  </Tooltip>
                </div>
              </div>
            </Fragment>
          )}
        </div>
      </div>
    </motion.div>
  );
};

export const Message = memo(MessageBase, (prev, next) => {
  return (
    prev.messageId === next.messageId &&
    prev.message.content === next.message.content &&
    prev.isPending === next.isPending &&
    equal(
      (prev.message as AssistantMessage).toolCalls,
      (next.message as AssistantMessage).toolCalls,
    ) &&
    (prev.message as AssistantMessage).provider?.agent.id ===
      (next.message as AssistantMessage).provider?.agent.id
  );
});

const variants = {
  initial: { opacity: 0, y: -25 },
  visible: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: 25 },
};

export function ToolCard({
  name,
  // args,
  status,
  toolRouterKey,
}: {
  args: ToolArgs;
  status: ToolStatusType;
  name: string;
  toolRouterKey: string;
}) {
  const renderStatus = () => {
    if (status === ToolStatusType.Complete) {
      return <ToolsIcon className="text-brand size-full" />;
    }
    if (status === ToolStatusType.Incomplete) {
      return <XCircle className="text-text-secondary size-full" />;
    }
    if (status === ToolStatusType.RequiresAction) {
      return <InfoIcon className="text-text-secondary size-full" />;
    }
    return <Loader2 className="text-brand size-full animate-spin" />;
  };

  const renderLabelText = () => {
    if (status === ToolStatusType.Complete) {
      return 'Tool Used';
    }
    if (status === ToolStatusType.Incomplete) {
      return 'Incomplete';
    }
    if (status === ToolStatusType.RequiresAction) {
      return 'Requires Action';
    }
    return 'Processing Tool';
  };

  return (
    <AnimatePresence initial={false} mode="popLayout">
      <motion.div
        animate="visible"
        exit="exit"
        initial="initial"
        transition={{ type: 'spring', duration: 0.3, bounce: 0 }}
        variants={variants}
      >
        <div className="flex items-center gap-1 p-[5px]">
          <div className="size-7 shrink-0 px-1.5">{renderStatus()}</div>
          <div className="flex items-center gap-1">
            <span className="text-text-secondary text-em-sm">
              {renderLabelText()}
            </span>
            <Link
              className="text-em-sm font-semibold text-white hover:underline"
              to={`/tools/${toolRouterKey}`}
            >
              {formatText(name)}
            </Link>
          </div>
        </div>
      </motion.div>
    </AnimatePresence>
  );
}
export function Reasoning({
  reasoning,
  status,
}: {
  reasoning: string;
  status?: TextStatus;
}) {
  const renderStatus = () => {
    if (status?.type === 'complete') {
      return <ReasoningIcon className="text-brand size-full" />;
    }
    if (status?.type === 'incomplete') {
      return <XCircle className="text-text-secondary size-full" />;
    }
    if (status?.type === 'running') {
      return null;
    }
    return null;
  };

  const renderReasoningText = () => {
    if (status?.type === 'complete') {
      return 'Reasoning';
    }
    return 'Thinking...';
  };

  return (
    <Accordion
      className="max-w-full space-y-1.5 self-baseline overflow-x-auto pb-3"
      collapsible
      type="single"
    >
      <AccordionItem
        className={cn(
          'bg-bg-dark border-divider overflow-hidden rounded-lg border',
          status?.type === 'running' &&
            'animate-pulse border-none bg-transparent',
        )}
        value="reasoning"
      >
        <AccordionTrigger
          className={cn(
            'inline-flex w-auto gap-3 p-[5px] no-underline hover:no-underline',
            'hover:bg-bg-tertiary transition-colors',
            status?.type === 'running' && 'p-0',
          )}
          hideArrow={status?.type === 'running'}
        >
          <AnimatePresence initial={false} mode="popLayout">
            <motion.div
              animate="visible"
              exit="exit"
              initial="initial"
              key={status?.type}
              transition={{ type: 'spring', duration: 0.3, bounce: 0 }}
              variants={variants}
            >
              <div
                className={cn(
                  'text-text-secondary flex items-center gap-1',
                  status?.type === 'running' && 'text-text-tertiary',
                )}
              >
                {renderStatus() && (
                  <div className="size-7 shrink-0 px-1.5">{renderStatus()}</div>
                )}
                <div className="flex items-center gap-1">
                  <span className="text-em-sm">{renderReasoningText()}</span>
                </div>
              </div>
            </motion.div>
          </AnimatePresence>
        </AccordionTrigger>
        <AccordionContent className="bg-bg-dark flex flex-col gap-1 rounded-b-lg px-3 pt-2 pb-3 text-sm">
          <span className="text-text-secondary break-words whitespace-pre-line">
            {reasoning}
          </span>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

export const GeneratedFiles = ({ toolCalls }: { toolCalls: ToolCall[] }) => {
  return (
    toolCalls.length > 0 &&
    toolCalls?.some(
      (tool) => !!tool.generatedFiles && tool.generatedFiles.length > 0,
    ) && (
      <div className="mt-4 space-y-1 py-4 pt-1.5">
        <span className="text-text-secondary text-em-sm">Generated Files</span>
        <div className="flex flex-wrap items-start gap-4 rounded-md">
          {toolCalls.map((tool) => {
            if (!tool.generatedFiles || !tool.generatedFiles.length)
              return null;
            return (
              <FileList
                className="mt-2"
                files={tool.generatedFiles.map((file) => ({
                  name: file.name,
                  path: file.path,
                  type: file.type,
                  size: file?.size,
                  content: file?.content,
                  blob: file?.blob,
                  id: file.id,
                  extension: file.extension,
                  mimeType: file.mimeType,
                  url: file.url,
                }))}
                key={tool.name}
              />
            );
          })}
        </div>
      </div>
    )
  );
};
