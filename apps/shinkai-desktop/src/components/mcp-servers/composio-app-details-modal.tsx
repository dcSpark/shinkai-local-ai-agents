import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  MarkdownText,
  Skeleton,
} from '@shinkai_network/shinkai-ui';
import { useState } from 'react';
import { useApp } from '../../lib/composio';

interface ComposioAppDetailsModalProps {
  isOpen: boolean;
  onClose: () => void;
  appId: string;
}

const DetailItem = ({
  label,
  value,
}: {
  label: string;
  value: string | undefined | null;
}) => (
  <div className="bg-bg-quaternary/50 hover:bg-bg-quaternary/70 rounded-lg p-3 transition-colors">
    <p className="text-text-secondary text-sm font-medium">{label}</p>
    <p className="mt-1 text-sm">{value || 'N/A'}</p>
  </div>
);

const LoadingSkeleton = () => (
  <div className="space-y-6">
    <div className="flex items-center gap-4">
      <Skeleton className="h-12 w-12 rounded-lg" />
      <Skeleton className="h-6 w-48 rounded" />
    </div>
    <Skeleton className="h-4 w-full rounded" />
    <div className="space-y-4">
      <Skeleton className="h-6 w-32 animate-pulse rounded" />
      <div className="grid grid-cols-2 gap-4">
        {[...Array(4)].map((_, i) => (
          <div key={i} className="space-y-2">
            <Skeleton className="h-4 w-24 rounded" />
            <Skeleton className="h-4 w-32 rounded" />
          </div>
        ))}
      </div>
    </div>
  </div>
);

const SectionTitle = ({ children }: { children: React.ReactNode }) => (
  <h3 className="text-text-default mb-4 text-lg font-semibold">{children}</h3>
);

export const ComposioAppDetailsModal = ({
  isOpen,
  onClose,
  appId,
}: ComposioAppDetailsModalProps) => {
  const { data: app, isLoading } = useApp(appId);
  const [imageError, setImageError] = useState(false);
  const { t } = useTranslation();

  return (
    <Dialog onOpenChange={(open: boolean) => !open && onClose()} open={isOpen}>
      <DialogContent className="flex max-h-[85vh] max-w-2xl flex-col overflow-hidden">
        {isLoading ? (
          <LoadingSkeleton />
        ) : (
          <>
            <DialogHeader className="flex-shrink-0">
              <DialogTitle className="flex items-center gap-4">
                <div className="bg-bg-quaternary relative h-12 w-12 overflow-hidden rounded-lg">
                  {!imageError ? (
                    <img
                      src={app?.meta.logo}
                      alt={`${app?.name} logo`}
                      className="h-full w-full object-cover"
                      onError={() => setImageError(true)}
                    />
                  ) : (
                    <div className="text-text-secondary flex h-full w-full items-center justify-center text-xs font-medium">
                      {app?.name?.[0]?.toUpperCase() || 'A'}
                    </div>
                  )}
                </div>
                <span className="text-text-default">{app?.name}</span>
              </DialogTitle>
              <DialogDescription className="text-text-secondary mt-2 leading-relaxed">
                {app?.meta.description}
              </DialogDescription>
            </DialogHeader>

            <div className="mt-6 space-y-8 overflow-y-auto pr-2">
              <section aria-labelledby="details-title">
                <SectionTitle>{t('mcpServers.composio.details')}</SectionTitle>
                <div className="grid grid-cols-2 gap-4">
                  <DetailItem
                    label={t('mcpServers.composio.created')}
                    value={
                      app?.meta.created_at
                        ? new Date(app.meta.created_at).toLocaleDateString(
                            undefined,
                            {
                              year: 'numeric',
                              month: 'long',
                              day: 'numeric',
                            },
                          )
                        : undefined
                    }
                  />
                  <DetailItem
                    label={t('mcpServers.composio.lastUpdated')}
                    value={
                      app?.meta.updated_at
                        ? new Date(app.meta.updated_at).toLocaleDateString(
                            undefined,
                            {
                              year: 'numeric',
                              month: 'long',
                              day: 'numeric',
                            },
                          )
                        : undefined
                    }
                  />
                  <DetailItem
                    label={t('mcpServers.composio.categories')}
                    value={app?.meta.categories
                      .map((cat) => cat.name)
                      .join(', ')}
                  />
                  <DetailItem
                    label={t('mcpServers.composio.toolsCount')}
                    value={app?.meta.tools_count.toString()}
                  />
                </div>
              </section>

              {app?.actions && app.actions.length > 0 && (
                <section aria-labelledby="actions-title">
                  <SectionTitle>
                    {t('mcpServers.composio.availableActions')}
                  </SectionTitle>
                  <div className="space-y-2">
                    {app.actions.map((action, index) => (
                      <div
                        key={index}
                        className="bg-bg-quaternary/50 hover:bg-bg-quaternary/70 rounded-lg p-3 transition-colors"
                      >
                        <p className="text-text-default font-semibold">
                          {action.name}
                        </p>
                        {action.description && (
                          <div className="text-text-secondary mt-2 text-sm">
                            <MarkdownText>{action.description}</MarkdownText>
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </section>
              )}

              {app?.metadata && (
                <section aria-labelledby="statistics-title" className="pb-4">
                  <SectionTitle>
                    {t('mcpServers.composio.statistics')}
                  </SectionTitle>
                  <div className="grid grid-cols-2 gap-4">
                    <DetailItem
                      label={t('mcpServers.composio.totalDownloads')}
                      value={app.metadata.totalDownloads}
                    />
                    <DetailItem
                      label={t('mcpServers.composio.activeUsers')}
                      value={app.metadata.activeUsers}
                    />
                    <DetailItem
                      label={t('mcpServers.composio.latestVersion')}
                      value={app.metadata.latestVersion}
                    />
                    <DetailItem
                      label={t('mcpServers.composio.updatedAt')}
                      value={app.metadata.lastUpdated}
                    />
                  </div>
                </section>
              )}
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
};
