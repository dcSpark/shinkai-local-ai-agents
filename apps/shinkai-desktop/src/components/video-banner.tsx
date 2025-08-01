import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import React from 'react';

interface VideoBannerProps {
  onClose?: () => void;
  title: string;
  videoUrl: string;
  duration: string;
}
export const SHINKAI_DOCS_URL = 'https://docs.shinkai.com';

export function VideoBanner({ title, videoUrl, duration }: VideoBannerProps) {
  const [isVideoDialogOpen, setIsVideoDialogOpen] = React.useState(false);
  const { t } = useTranslation();

  return (
    <>
      <div
        className={cn(
          'animate-scale-in group relative cursor-pointer overflow-hidden rounded-2xl border border-white/10',
          'border-bg-quaternary bg-bg-quaternary w-full flex-shrink-0 p-2.5 transition-all',
        )}
        onClick={() => setIsVideoDialogOpen(true)}
      >
        <div className="aspect-video w-full overflow-hidden">
          <img
            alt={title}
            className="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
            src={'./video-portrait.png'}
          />
        </div>
        <div className="absolute inset-0 bg-gradient-to-t from-black via-black/60 to-black/5 transition-opacity duration-300 group-hover:opacity-90" />
        <div className="absolute right-0 bottom-0 left-0 p-5 pt-10">
          <h3 className="mb-1 truncate text-left text-base font-medium text-white">
            {title}
          </h3>
          <div className="flex items-center justify-between">
            <span className="inline-flex rounded-full border border-white/10 bg-white/10 px-2.5 py-1">
              <span className="text-xs font-medium text-white/80">
                {duration}
              </span>
            </span>
          </div>
        </div>
      </div>

      <Dialog onOpenChange={setIsVideoDialogOpen} open={isVideoDialogOpen}>
        <DialogContent className="sm:max-w-[85vw]">
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
          </DialogHeader>
          <div className="relative aspect-video w-full overflow-hidden rounded-lg">
            <video className="h-full w-full" controls src={videoUrl}>
              {t('videoBanner.unsupported')}
            </video>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
