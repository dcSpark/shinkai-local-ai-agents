import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type ComponentProps, type ReactNode } from 'react';
import { ErrorBoundary } from 'react-error-boundary';

/**
 * Fallback component shown when the Prism editor crashes
 */
const PrismEditorFallback = ({ resetErrorBoundary }: { error: Error; resetErrorBoundary: () => void }) => {
  const { t } = useTranslation();
  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-4 p-4 text-center text-xs">
      <p>{t('playgroundTool.prismError')}</p>
      <button
        className="rounded-md bg-gray-600 px-3 py-1 hover:bg-gray-500"
        onClick={resetErrorBoundary}
      >
        {t('common.tryAgain')}
      </button>
    </div>
  );
};

/**
 * Error boundary specifically for Prism editor components to prevent
 * crashes from affecting the rest of the application
 */
export const PrismErrorBoundary = ({
  children,
  onError,
}: {
  children: ReactNode;
  onError?: ComponentProps<typeof ErrorBoundary>['onError'];
}) => {
  return (
    <ErrorBoundary
      FallbackComponent={PrismEditorFallback}
      onError={(error, errorInfo) => {
        console.error('Prism editor error:', error, errorInfo);
        if (onError) {
          onError(error, errorInfo);
        }
      }}
    >
      {children}
    </ErrorBoundary>
  );
};

export default PrismErrorBoundary;
