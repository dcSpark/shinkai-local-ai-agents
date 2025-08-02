import * as React from 'react';

import { type RefCallBack } from 'react-hook-form';
import { cn } from '../utils';

const DEFAULT_MIN_TEXTAREA_HEIGHT = 32;
const DEFAULT_MAX_TEXTAREA_HEIGHT = 300;

export type TextareaProps =
  React.TextareaHTMLAttributes<HTMLTextAreaElement> & {
    minHeight?: number;
    maxHeight?: number;
    resize?: 'none' | 'both' | 'horizontal' | 'vertical';
    ref?: React.RefObject<HTMLTextAreaElement | null> | RefCallBack;
  };

const Textarea = ({
  className,
  minHeight = DEFAULT_MIN_TEXTAREA_HEIGHT,
  maxHeight = DEFAULT_MAX_TEXTAREA_HEIGHT,
  resize = 'none',
  ref,
  ...props
}: TextareaProps) => {
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);

  React.useLayoutEffect(() => {
    // Reset height - important to shrink on delete
    if (!textareaRef.current) return;
    textareaRef.current.style.height = 'inherit';
    // Set height
    textareaRef.current.style.height = `${Math.max(
      textareaRef.current.scrollHeight + 2,
      minHeight,
    )}px`;

    if (props.autoFocus !== undefined && props.autoFocus) {
      textareaRef.current.focus();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.value, minHeight, maxHeight]);

  return (
    <textarea
      className={cn(
        'bg-bg-secondary placeholder:!text-text-placeholder border-input focus-visible:ring-border-input-focus flex w-full rounded-md border px-4 py-2 pt-7 text-sm break-words focus-visible:ring-1 focus-visible:outline-hidden focus-visible:ring-inset disabled:cursor-not-allowed disabled:opacity-50',
        className,
      )}
      ref={ref || textareaRef}
      style={{
        minHeight: `${minHeight}px`,
        maxHeight: resize === 'vertical' ? undefined : `${maxHeight}px`,
        resize: resize,
      }}
      {...props}
    />
  );
};
Textarea.displayName = 'Textarea';

export { Textarea };
