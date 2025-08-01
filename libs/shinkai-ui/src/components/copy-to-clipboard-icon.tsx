import { CheckCircle2, CopyIcon } from 'lucide-react';
import React, { cloneElement, type ReactElement } from 'react';

import { useCopyClipboard } from '../hooks';
import { cn } from '../utils';
import { Button } from './button';

type CopyToClipboardIconProps = {
  string?: string;
  children?: ReactElement<{
    className?: string;
    onClick?: () => void;
  }>;
  className?: string;
  onCopyClipboard?: () => void;
  asChild?: boolean;
};

const CopyToClipboardIcon = ({
  string,
  children,
  className,
  onCopyClipboard,
  asChild = false,
}: CopyToClipboardIconProps) => {
  const { isCopied, onCopy } = useCopyClipboard({
    string,
    onCopyClipboard,
  });

  const ClipboardIcon = isCopied ? CheckCircle2 : CopyIcon;

  if (asChild && children) {
    return cloneElement(children, {
      onClick: onCopy,
      className: cn(children.props?.className, className),
    });
  }

  return (
    <Button
      className={cn(
        'text-text-secondary hover:bg-bg-tertiary bg-bg-tertiary flex h-8 w-8 gap-2 rounded-lg text-xs font-normal transition-colors hover:text-white',
        className,
      )}
      disabled={!string}
      onClick={onCopy}
      size={'icon'}
      type="button"
      variant="ghost"
    >
      <ClipboardIcon
        className={cn('h-3.5 w-3.5', isCopied && 'text-green-500')}
      />
      {children}
    </Button>
  );
};

export { CopyToClipboardIcon, useCopyClipboard };
