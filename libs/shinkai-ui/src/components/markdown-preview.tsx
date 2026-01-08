import { type ComponentPropsWithoutRef, type FC, memo } from 'react';
import { defaultRehypePlugins, Streamdown } from 'streamdown';

import { cn } from '../utils';

const isImageUrl = (url: string): boolean => {
  try {
    const urlObj = new URL(url);
    const pathname = urlObj.pathname.toLowerCase();
    const imageExtensions = [
      '.png',
      '.jpg',
      '.jpeg',
      '.gif',
      '.bmp',
      '.webp',
      '.svg',
      '.ico',
      '.tiff',
      '.tif',
      '.avif',
      '.heic',
      '.heif',
      '.jfif',
      '.pjpeg',
      '.pjp',
    ];
    return imageExtensions.some((ext) => pathname.endsWith(ext));
  } catch {
    return false;
  }
};

const isVideoUrl = (url: string): boolean => {
  try {
    const urlObj = new URL(url);
    const pathname = urlObj.pathname.toLowerCase();
    const videoExtensions = ['.mp4', '.webm', '.ogg', '.mov', '.avi', '.mkv'];
    return videoExtensions.some((ext) => pathname.endsWith(ext));
  } catch {
    return false;
  }
};

const MediaAwareLink: FC<ComponentPropsWithoutRef<'a'>> = ({
  href,
  children,
  className,
  ...props
}) => {
  const isImage = href && isImageUrl(href);
  const isVideo = href && isVideoUrl(href);

  // Regular link for non-media URLs
  if (!href || (!isImage && !isVideo)) {
    return (
      <a
        className={cn(
          'font-medium text-blue-400 underline underline-offset-4',
          className,
        )}
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        {...props}
      >
        {children}
      </a>
    );
  }

  // Video rendering
  if (isVideo) {
    return (
      <span className="my-4 inline-block w-full">
        {/** biome-ignore lint/a11y/useMediaCaption: <ignore> */}
        <video
          className="h-auto max-w-full rounded-lg border border-gray-600 shadow-sm"
          controls
          preload="metadata"
        >
          <source src={href} type={`video/${href.split('.').pop() ?? 'mp4'}`} />
          Your browser does not support the video tag.
        </video>
        {children && typeof children === 'string' && children !== href && (
          <p className="text-text-secondary mt-2 text-sm italic">{children}</p>
        )}
      </span>
    );
  }

  // Image rendering
  return (
    <span className="my-4 inline-block">
      {/** biome-ignore lint/a11y/useKeyWithClickEvents: <ignore> */}
      <img
        alt={typeof children === 'string' ? children : 'Image'}
        className="h-auto max-w-full cursor-pointer rounded-lg border border-gray-600 shadow-sm transition-opacity hover:opacity-90"
        loading="lazy"
        onClick={() => window.open(href, '_blank')}
        src={href}
        onError={(e) => {
          // Fallback to regular link if image fails to load
          const target = e.target as HTMLImageElement;
          const parent = target.parentElement;
          if (parent) {
            parent.innerHTML = `<a href="${href}" target="_blank" rel="noopener noreferrer" class="font-medium text-blue-400 underline underline-offset-4">${children}</a>`;
          }
        }}
      />
      {children && typeof children === 'string' && children !== href && (
        <p className="text-text-secondary mt-2 text-sm italic">{children}</p>
      )}
    </span>
  );
};

export type MarkdownTextProps = {
  children: string;
  className?: string;
};

const MarkdownTextBase = ({ children, className }: MarkdownTextProps) => {
  return (
    <Streamdown
      className={cn(
        'size-full',
        '[&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
        '[&_code]:break-words [&_code]:whitespace-pre-wrap',
        '[&_pre]:max-w-full [&_pre]:overflow-x-auto',
        className,
      )}
      rehypePlugins={[defaultRehypePlugins.katex, defaultRehypePlugins.harden]}
      components={{
        a: MediaAwareLink,
      }}
      mermaid={{
        config: {
          theme: 'dark',
        },
      }}
      controls={{
        table: false, // Show table download button
        code: true, // Show code copy button
        mermaid: {
          download: false, // Show mermaid download button
          copy: true, // Show mermaid copy button
          fullscreen: false, // Show mermaid fullscreen button
          panZoom: true, // Show mermaid pan/zoom controls
        },
      }}
      shikiTheme={['github-dark', 'github-dark']}
    >
      {children}
    </Streamdown>
  );
};

export const MarkdownText = memo(
  MarkdownTextBase,
  (prev, next) =>
    prev.children === next.children && prev.className === next.className,
);
