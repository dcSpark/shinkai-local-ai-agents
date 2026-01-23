import { createCodePlugin } from '@streamdown/code';
import { math } from '@streamdown/math';
import { mermaid } from '@streamdown/mermaid';
import { type ComponentPropsWithoutRef, type FC, memo } from 'react';
import { Streamdown } from 'streamdown';
import 'katex/dist/katex.min.css';

import { cn } from '../utils';

const codePlugin = createCodePlugin({
  themes: ['github-dark', 'github-dark'],
});

/**
 * Preprocesses markdown content to fix common Mermaid syntax issues.
 * Specifically fixes curly braces inside node labels which Mermaid
 * incorrectly interprets as diamond/rhombus shapes.
 *
 * Example: `F[Update: {Real Time}]` becomes `F["Update: {Real Time}"]`
 */
const preprocessMermaidContent = (content: string): string => {
  // Match mermaid code blocks
  return content.replace(
    /(```mermaid\s*\n)([\s\S]*?)(```)/g,
    (_match, start, mermaidCode, end) => {
      // Fix node labels containing curly braces
      // Match patterns like [text with {braces}] and wrap content in quotes
      const fixedCode = mermaidCode.replace(
        /\[([^\]"]*\{[^\]]*)\]/g,
        (_nodeMatch: string, nodeContent: string) => {
          // If already quoted, don't modify
          if (nodeContent.startsWith('"') && nodeContent.endsWith('"')) {
            return `["${nodeContent}"]`;
          }
          // Wrap the content in quotes to escape special characters
          return `["${nodeContent}"]`;
        },
      );
      return start + fixedCode + end;
    },
  );
};


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
  const processedContent = preprocessMermaidContent(children);

  return (
    <Streamdown
      className={cn(
        'size-full',
        '[&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
        '[&_code]:break-words',
        '[&_pre]:max-w-full [&_pre]:overflow-x-auto',
        '[&_pre_code>span.block]:before:content-none',
        className,
      )}
      plugins={{ code: codePlugin, math, mermaid }}
      components={{
        a: MediaAwareLink,
      }}
      mermaid={{
        config: {
          theme: 'dark',
        },
      }}
      controls={{
        table: false,
        code: true,
        mermaid: {
          download: false,
          copy: true,
          fullscreen: false,
          panZoom: true,
        },
      }}
    >
      {processedContent}
    </Streamdown>
  );
};

export const MarkdownText = memo(
  MarkdownTextBase,
  (prev, next) =>
    prev.children === next.children && prev.className === next.className,
);
