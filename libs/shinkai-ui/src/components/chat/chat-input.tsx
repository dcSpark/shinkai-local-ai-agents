import * as React from 'react';

import { cn } from '../../utils';

export interface ChatInputProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  onSend?: () => void;
  ref: React.RefObject<HTMLTextAreaElement | null>;
}

export const useAutoResizeTextarea = (
  ref: React.RefObject<HTMLTextAreaElement | null>,
  value: string | number | readonly string[] | undefined,
  autoResize = true, //later to unify
) => {
  const textAreaRef = React.useRef<HTMLTextAreaElement>(null);

  React.useImperativeHandle(ref, () => textAreaRef.current!);

  React.useEffect(() => {
    const ref = textAreaRef?.current;

    const updateTextareaHeight = () => {
      if (ref && autoResize) {
        ref.style.height = 'auto';
        ref.style.height = ref?.scrollHeight + 'px';
      }
    };

    updateTextareaHeight();

    ref?.addEventListener('input', updateTextareaHeight);
    return () => ref?.removeEventListener('input', updateTextareaHeight);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  return { textAreaRef };
};

const ChatInputBase = ({
  className,
  onSend,
  onKeyDown,
  ref,
  ...props
}: ChatInputProps) => {
  const { textAreaRef } = useAutoResizeTextarea(ref, props.value);

  return (
    <textarea
      className={cn(
        'placeholder:text-text-placeholder flex max-h-[40vh] min-h-[80px] w-full resize-none overflow-y-auto border-none bg-transparent px-3 py-2 text-base leading-normal break-words focus:outline-hidden focus-visible:ring-0 focus-visible:outline-hidden disabled:cursor-not-allowed disabled:opacity-50',
        className,
      )}
      id="chat-input"
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (event.key === 'Enter' && !event.shiftKey) {
          event.preventDefault();
          onSend?.();
        }
      }}
      ref={textAreaRef}
      spellCheck={false}
      {...props}
    />
  );
};

ChatInputBase.displayName = 'ChatInput';
export const ChatInput = React.memo(ChatInputBase);
