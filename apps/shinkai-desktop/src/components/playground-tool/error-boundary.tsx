import { ReloadIcon } from '@radix-ui/react-icons';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { Button } from '@shinkai_network/shinkai-ui';

export function ToolErrorFallback({
  error,
  resetErrorBoundary,
}: {
  error: Error;
  resetErrorBoundary: (...args: any[]) => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      className="flex flex-col items-center gap-4 bg-red-900/20 px-3 py-4 text-xs text-red-400"
      role="alert"
    >
      <div className="space-y-2 text-center">
        <p>{t('playgroundTool.metadataError')}</p>
        <pre className="whitespace-break-spaces break-words px-4">{error.message}</pre>
      </div>
      <Button
        className="bg-gray-400 text-white"
        onClick={() => resetErrorBoundary()}
        size="xs"
        type="button"
        variant="outline"
      >
        <ReloadIcon className="size-3.5" />
        {t('common.tryAgain')}
      </Button>
    </div>
  );
}
