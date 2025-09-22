import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { Paperclip } from 'lucide-react';
import * as React from 'react';

import { actionButtonClassnames } from '../conversation-footer';

type FileUploadInputProps = {
  disabled?: boolean;
  inputProps: React.InputHTMLAttributes<HTMLInputElement>;
  onClick: () => void;
  showLabel?: boolean;
};

function FileSelectionActionBarBase({
  onClick,
  inputProps,
  disabled,
  showLabel,
}: FileUploadInputProps) {
  const { t } = useTranslation();

  if (!showLabel) {
    return (
      <>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className={cn(actionButtonClassnames, 'p-2')}
              disabled={disabled}
              onClick={onClick}
              type="button"
            >
              <Paperclip className="h-full w-full" />
            </button>
          </TooltipTrigger>
          <TooltipPortal>
            <TooltipContent align="center" side="top">
              {t('common.uploadFile')} <br />
              {t('common.uploadAFileDescription')}
            </TooltipContent>
          </TooltipPortal>
        </Tooltip>
        <input {...inputProps} disabled={disabled} />
      </>
    );
  }
  return (
    <>
      <button
        className={cn(actionButtonClassnames, 'w-auto justify-start gap-2.5')}
        disabled={disabled}
        onClick={onClick}
        type="button"
      >
        <Paperclip className="size-4" />
        <span className="">{t('common.uploadFile')}</span>
      </button>

      <input {...inputProps} disabled={disabled} />
    </>
  );
}
export const FileSelectionActionBar = React.memo(
  FileSelectionActionBarBase,
  (prevProps, nextProps) => {
    return prevProps.disabled === nextProps.disabled;
  },
);
