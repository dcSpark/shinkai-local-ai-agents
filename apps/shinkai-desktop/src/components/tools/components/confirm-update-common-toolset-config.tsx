import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@shinkai_network/shinkai-ui';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';

interface ConfirmToolsetUpdateDialogProps {
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  onConfirmUpdateTool: () => void;
  onConfirmUpdateToolset: () => void;
  toolName?: string;
  affectedToolNames?: string[];
  toolSetName?: string;
  isSettingCommonToolsetConfig: boolean;
  isUpdatingTool: boolean;
}

export function ConfirmToolsetUpdateDialog({
  isOpen,
  onOpenChange,
  onConfirmUpdateTool,
  onConfirmUpdateToolset,
  toolName,
  affectedToolNames,
  toolSetName,
  isSettingCommonToolsetConfig,
  isUpdatingTool,
}: ConfirmToolsetUpdateDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog onOpenChange={onOpenChange} open={isOpen}>
      <DialogContent showCloseButton={true}>
        <DialogHeader>
          <DialogTitle>
            {t('tools.configuration.updateConfig', {
              toolName: toolName ? `"${toolName}"` : 'this tool',
            })}
          </DialogTitle>
          <DialogDescription className="text-sm">
            {t('tools.configuration.updateConfigDescription2', {
              toolSetName: toolSetName
                ? formatText(toolSetName ?? '')
                : 'this tool',
            })}
          </DialogDescription>
        </DialogHeader>
        {affectedToolNames && affectedToolNames.length > 0 && (
          <div className="border-divider bg-bg-tertiary space-y-2 rounded-lg border p-3 text-sm">
            <p className="font-medium">
              {t('tools.configuration.followingToolsAffected')}
            </p>
            <ul className="text-text-secondary list-disc space-y-1 pl-5">
              {affectedToolNames.map((name) => (
                <li key={name}>{name}</li>
              ))}
            </ul>
          </div>
        )}
        <DialogFooter className="flex flex-col gap-2 pt-4 sm:justify-center">
          <Button
            className="flex-1"
            disabled={isSettingCommonToolsetConfig}
            isLoading={isSettingCommonToolsetConfig}
            onClick={onConfirmUpdateToolset}
            size="md"
          >
            {t('tools.configuration.updateAllToolsInSet', {
              toolSetName: formatText(toolSetName?.split('_')[0] ?? ''),
            })}
          </Button>
          <Button
            className="flex-1"
            disabled={isUpdatingTool}
            isLoading={isUpdatingTool}
            onClick={onConfirmUpdateTool}
            size="md"
            variant="outline"
          >
            {t('tools.configuration.updateOnlyThisTool')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
