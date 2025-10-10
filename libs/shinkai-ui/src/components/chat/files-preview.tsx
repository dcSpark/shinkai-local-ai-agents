import { DialogClose } from '@radix-ui/react-dialog';
import {
  type Attachment,
  FileTypeSupported,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import * as fs from '@tauri-apps/plugin-fs';
import { BaseDirectory } from '@tauri-apps/plugin-fs';
import { partial } from 'filesize';
import { AnimatePresence, motion } from 'framer-motion';
import { CircleSlashIcon, XIcon } from 'lucide-react';
import React, { useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';

import {
  DownloadIcon,
  ExternalLinkIcon,
  fileIconMap,
  FileTypeIcon,
  PaperClipIcon,
} from '../../assets/icons';
import { getFileExt } from '../../helpers/file';
import { cn } from '../../utils';
import { Avatar, AvatarFallback, AvatarImage } from '../avatar';
import { Button } from '../button';
import { Dialog, DialogContent } from '../dialog';
import {
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '../tooltip';
import { SqlitePreview } from './sqlite-preview';

export type FileListProps = {
  files: Attachment[];
  className?: string;
};

export const isImageFile = (file: string) => {
  return file.match(/\.(jpg|jpeg|png|gif)$/i);
};

const size = partial({ standard: 'jedec' });

const resolveNodeStorageRelativePath = (filePath?: string | null) => {
  if (!filePath) return null;

  let normalizedPath = filePath.trim();
  if (!normalizedPath) return null;

  if (normalizedPath.startsWith('shinkai://file/')) {
    normalizedPath = normalizedPath.slice('shinkai://file/'.length);
  }

  if (normalizedPath.startsWith('@@')) {
    normalizedPath = normalizedPath.slice(2);
    const segments = normalizedPath.split('/').filter(Boolean);

    if (segments.length === 0) return null;

    segments.shift();

    if (segments[0] && segments[0].toLowerCase() === 'main') {
      segments.shift();
    }

    normalizedPath = segments.join('/');
  }

  normalizedPath = normalizedPath.replace(/^\/+/, '');

  if (!normalizedPath) return null;

  const segments = normalizedPath.split('/').filter(Boolean);
  if (segments.length === 0) return null;

  const firstSegment = segments[0];

  if (
    firstSegment === 'tools_storage' ||
    firstSegment === 'filesystem' ||
    firstSegment === 'internal_tools_storage' ||
    firstSegment === 'main_db'
  ) {
    return segments.join('/');
  }

  if (firstSegment === 'global-cache') {
    return ['tools_storage', ...segments].join('/');
  }

  if (firstSegment.startsWith('jobid_')) {
    return ['tools_storage', ...segments].join('/');
  }

  return ['tools_storage', ...segments].join('/');
};

const ImagePreview = ({
  name,
  size,
  url,
  onFullscreen,
}: Pick<Attachment, 'name' | 'size' | 'url'> & {
  onFullscreen: (open: boolean) => void;
}) => (
  <button
    className="border-divider hover:bg-bg-secondary flex h-14 w-full max-w-[210px] min-w-[210px] shrink-0 cursor-pointer items-center gap-2 rounded-md border py-1.5 pr-1.5 pl-2 text-left"
    onClick={() => onFullscreen(true)}
  >
    <Avatar className="bg-bg-quaternary text-text-default flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-xs transition-colors">
      <AvatarImage
        alt={name}
        className="border-divider aspect-square h-full w-full rounded-xs border object-cover"
        src={url}
      />
      <AvatarFallback>
        <CircleSlashIcon className="text-text-secondary h-4 w-4" />
      </AvatarFallback>
    </Avatar>
    <FileInfo fileName={name} fileSize={size} />
  </button>
);

const FileInfo = ({
  fileSize,
  fileName,
}: {
  fileSize?: number;
  fileName: string;
}) => (
  <div className="text-text-secondary text-em-sm grid flex-1 -translate-x-px gap-1 py-0.5 leading-none">
    <div className="text-text-default truncate overflow-hidden font-medium">
      {decodeURIComponent(fileName.split('/').at(-1) ?? '')}
    </div>
    {fileSize && (
      <div className="text-text-secondary line-clamp-1 aspect-auto font-normal">
        {size(fileSize)}
      </div>
    )}
  </div>
);

const MAX_CSV_ROWS = 120;
const MAX_CSV_COLUMNS = 20;

const parseCsvRows = (input: string): string[][] => {
  const rows: string[][] = [];
  let currentRow: string[] = [];
  let currentValue = '';
  let inQuotes = false;
  let lastCharWasNewline = false;

  const commitValue = () => {
    currentRow.push(currentValue);
    currentValue = '';
  };

  const commitRow = () => {
    commitValue();
    rows.push(currentRow);
    currentRow = [];
  };

  for (let index = 0; index < input.length; index += 1) {
    const char = input[index];

    if (inQuotes) {
      if (char === '"') {
        if (input[index + 1] === '"') {
          currentValue += '"';
          index += 1;
        } else {
          inQuotes = false;
        }
      } else {
        currentValue += char;
      }
      lastCharWasNewline = false;
      continue;
    }

    if (char === '"') {
      inQuotes = true;
      lastCharWasNewline = false;
      continue;
    }

    if (char === ',') {
      commitValue();
      lastCharWasNewline = false;
      continue;
    }

    if (char === '\r' || char === '\n') {
      commitRow();
      lastCharWasNewline = true;
      if (char === '\r' && input[index + 1] === '\n') {
        index += 1;
      }
      continue;
    }

    currentValue += char;
    lastCharWasNewline = false;
  }

  if (inQuotes) {
    commitValue();
    rows.push(currentRow);
  } else if (
    currentValue !== '' ||
    currentRow.length > 0 ||
    (!lastCharWasNewline && input.length > 0)
  ) {
    commitValue();
    rows.push(currentRow);
  }

  if (rows.length === 1 && rows[0]?.length === 1 && rows[0]?.[0] === '') {
    return [];
  }

  return rows;
};

const CsvPreview = ({
  blob,
  content,
  name,
}: Pick<Attachment, 'blob' | 'content' | 'name'>) => {
  const [csvText, setCsvText] = useState<string | null>(content ?? null);
  const [rows, setRows] = useState<string[][]>([]);
  const [isLoading, setIsLoading] = useState<boolean>(!content && !!blob);
  const [error, setError] = useState<string | null>(null);
  const [isRowTruncated, setIsRowTruncated] = useState(false);
  const [isColumnTruncated, setIsColumnTruncated] = useState(false);

  useEffect(() => {
    let isCancelled = false;

    if (content !== undefined && content !== null) {
      setCsvText(content);
      setIsLoading(false);
      setError(null);
      return;
    }

    if (!blob) {
      setCsvText(null);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);

    blob
      .text()
      .then((value) => {
        if (isCancelled) return;
        setCsvText(value);
        setError(null);
      })
      .catch((err) => {
        if (isCancelled) return;
        console.error('Failed to read CSV blob:', err);
        setError('Unable to preview this CSV file');
      })
      .finally(() => {
        if (!isCancelled) {
          setIsLoading(false);
        }
      });

    return () => {
      isCancelled = true;
    };
  }, [blob, content]);

  useEffect(() => {
    if (csvText === null) {
      setRows([]);
      setIsRowTruncated(false);
      setIsColumnTruncated(false);
      return;
    }

    try {
      const parsedRows = parseCsvRows(csvText);

      if (!parsedRows?.length) {
        setRows([]);
        setIsRowTruncated(false);
        setIsColumnTruncated(false);
        return;
      }

      const limitedRows = parsedRows
        .slice(0, Math.min(parsedRows.length, MAX_CSV_ROWS + 1))
        .map((row) => row.slice(0, MAX_CSV_COLUMNS));

      setRows(limitedRows);
      setIsRowTruncated(parsedRows.length > limitedRows.length);
      setIsColumnTruncated(
        parsedRows.some((row) => row.length > MAX_CSV_COLUMNS),
      );
      setError(null);
    } catch (err) {
      console.error('Failed to parse CSV content:', err);
      setError('Unable to preview this CSV file');
      setRows([]);
    }
  }, [csvText]);

  if (isLoading) {
    return (
      <div className="text-text-secondary flex h-full w-full items-center justify-center">
        <span>Loading preview...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="text-text-secondary flex h-full w-full items-center justify-center">
        <span>{error}</span>
      </div>
    );
  }

  if (csvText === null) {
    return (
      <div className="text-text-secondary flex h-full w-full items-center justify-center">
        <span>Preview not available for this CSV file</span>
      </div>
    );
  }

  if (rows.length === 0) {
    return (
      <div className="text-text-secondary flex h-full w-full items-center justify-center">
        <span>No data in this CSV file</span>
      </div>
    );
  }

  const columnCount = rows.reduce(
    (max, row) => (row.length > max ? row.length : max),
    0,
  );
  const [headerRow, ...dataRows] = rows;

  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <div className="border-divider bg-bg-secondary text-text-secondary flex items-center justify-between gap-2 border-b px-4 py-2 text-xs">
        <span className="truncate" title={name}>
          CSV Preview
        </span>
        {(isRowTruncated || isColumnTruncated) && (
          <span>
            Showing first {Math.max(rows.length - 1, 0)} rows
            {isColumnTruncated ? ` and ${MAX_CSV_COLUMNS} columns` : ''}
          </span>
        )}
      </div>
      <div className="bg-bg-default relative flex-1 overflow-auto">
        <table className="min-w-full border-collapse text-left text-xs">
          <thead className="sticky top-0 z-10 bg-bg-tertiary">
            <tr>
              {Array.from({ length: columnCount }).map((_, index) => (
                <th
                  className="border-divider text-text-default border px-3 py-2 font-semibold"
                  key={`header-${index}`}
                >
                  {headerRow?.[index] ?? ''}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {dataRows.map((row, rowIndex) => (
              <tr
                className={cn(
                  rowIndex % 2 === 0 ? 'bg-bg-default' : 'bg-bg-secondary',
                )}
                key={`row-${rowIndex}`}
              >
                {Array.from({ length: columnCount }).map((_, columnIndex) => (
                  <td
                    className="border-divider text-text-secondary whitespace-pre-line border px-3 py-2 align-top"
                    key={`cell-${rowIndex}-${columnIndex}`}
                  >
                    {row?.[columnIndex] ?? ''}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {(isRowTruncated || isColumnTruncated) && (
        <div className="text-text-secondary border-divider bg-bg-secondary border-t px-4 py-2 text-xs">
          Preview truncated for performance. Download the file to view it in full.
        </div>
      )}
    </div>
  );
};

const themeVariables = [
  '--color-bg-default',
  '--color-bg-secondary',
  '--color-text-default',
  '--color-text-secondary',
  '--color-divider',
  '--color-brand-500',
  '--font-family-inter',
] as const;

const fallbackThemeValues: Record<(typeof themeVariables)[number], string> = {
  '--color-bg-default': '#1e1e1e',
  '--color-bg-secondary': '#262525',
  '--color-text-default': '#f7f5f5',
  '--color-text-secondary': '#b2aeae',
  '--color-divider': 'rgba(255, 255, 255, 0.1)',
  '--color-brand-500': '#fe6162',
  '--font-family-inter':
    "Inter, ui-sans-serif, system-ui, sans-serif, 'Apple Color Emoji', 'Segoe UI Emoji', 'Segoe UI Symbol', 'Noto Color Emoji'",
};

const HtmlPreview = ({
  blob,
  content,
  name,
  url,
}: Pick<Attachment, 'blob' | 'content' | 'name' | 'url'>) => {
  const [htmlContent, setHtmlContent] = useState<string | null>(
    content ?? null,
  );
  const [isLoading, setIsLoading] = useState<boolean>(!content && !!blob);
  const [error, setError] = useState<string | null>(null);
  const [themeCss, setThemeCss] = useState<string>(
    themeVariables
      .map((variable) => `${variable}: ${fallbackThemeValues[variable]};`)
      .join(' '),
  );

  useEffect(() => {
    let isCancelled = false;

    if (content !== undefined && content !== null) {
      setHtmlContent(content);
      setIsLoading(false);
      setError(null);
      return;
    }

    if (!blob) {
      setHtmlContent(null);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);

    blob
      .text()
      .then((value) => {
        if (isCancelled) return;
        setHtmlContent(value);
        setError(null);
      })
      .catch((err) => {
        if (isCancelled) return;
        console.error('Failed to read HTML blob:', err);
        setError('Unable to preview this HTML file');
      })
      .finally(() => {
        if (!isCancelled) {
          setIsLoading(false);
        }
      });

    return () => {
      isCancelled = true;
    };
  }, [blob, content]);

  useEffect(() => {
    try {
      const root = document.documentElement;
      const computedStyle = getComputedStyle(root);
      const cssVariables = themeVariables
        .map((variable) => {
          const value = computedStyle.getPropertyValue(variable).trim();
          const resolved = value || fallbackThemeValues[variable];
          return resolved ? `${variable}: ${resolved};` : null;
        })
        .filter(Boolean)
        .join(' ');

      if (cssVariables) {
        setThemeCss(cssVariables);
      }
    } catch (err) {
      console.warn('Failed to resolve theme variables for HTML preview:', err);
    }
  }, []);

  const styleTag = useMemo(() => {
    const baseStyles = `:root { ${themeCss} }
body { margin: 0; padding: 1.5rem; background: var(--color-bg-default); color: var(--color-text-default); font-family: var(--font-family-inter); line-height: 1.6; }
a { color: var(--color-brand-500); }
table { width: 100%; border-collapse: collapse; }
th, td { border: 1px solid var(--color-divider); padding: 0.5rem 0.75rem; text-align: left; }
pre { background: var(--color-bg-secondary); padding: 0.75rem 1rem; border-radius: 0.75rem; overflow: auto; }
code { font-family: inherit; }
img, video, iframe { max-width: 100%; height: auto; }
h1, h2, h3, h4, h5, h6 { color: var(--color-text-default); }
p { color: var(--color-text-secondary); }
`;

    return `<style>${baseStyles}</style>`;
  }, [themeCss]);

  const htmlWithStyles = useMemo(() => {
    if (htmlContent === null) return null;
    const trimmed = htmlContent.trim();

    if (!trimmed) {
      return `<!doctype html><html><head>${styleTag}</head><body></body></html>`;
    }

    const hasHtmlTag = /<html[\s>]/i.test(trimmed);
    const hasHeadTag = /<head[\s>]/i.test(trimmed);

    if (hasHtmlTag) {
      if (hasHeadTag) {
        return trimmed.replace(/<head[^>]*>/i, (match) => `${match}${styleTag}`);
      }

      return trimmed.replace(
        /<html[^>]*>/i,
        (match) => `${match}<head>${styleTag}</head>`,
      );
    }

    return `<!doctype html><html><head>${styleTag}</head><body>${trimmed}</body></html>`;
  }, [htmlContent, styleTag]);

  if (isLoading) {
    return (
      <div className="text-text-secondary flex h-full w-full items-center justify-center">
        <span>Loading preview...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="text-text-secondary flex h-full w-full items-center justify-center">
        <span>{error}</span>
      </div>
    );
  }

  if (htmlContent === null && !url) {
    return (
      <div className="text-text-secondary flex h-full w-full items-center justify-center">
        <span>Preview not available for this HTML file</span>
      </div>
    );
  }

  return (
    <div className="flex h-full w-full items-center justify-center">
      <iframe
        className="bg-bg-quaternary text-text-default h-full w-full"
        sandbox="allow-same-origin"
        src={htmlContent === null ? url : undefined}
        srcDoc={htmlWithStyles ?? undefined}
        title={name}
      />
    </div>
  );
};

type FileContentViewerProps = Pick<
  Attachment,
  'name' | 'url' | 'content' | 'type' | 'blob'
>;

export const FileContentViewer: React.FC<FileContentViewerProps> = ({
  name,
  content,
  url,
  type,
  blob,
}) => {
  switch (type) {
    case FileTypeSupported.Text: {
      return (
        <pre className="bg-bg-dark h-full overflow-auto p-4 pt-10 font-mono text-xs break-words whitespace-pre-wrap">
          {content}
        </pre>
      );
    }
    case FileTypeSupported.Image: {
      return (
        <div className="flex h-full w-full items-center justify-center">
          <img
            alt={name}
            className="max-h-full max-w-full object-contain"
            src={url}
          />
        </div>
      );
    }
    case FileTypeSupported.Video: {
      return (
        <div className="flex h-full w-full items-center justify-center">
          <video className="max-h-full max-w-full" controls src={url}>
            Your browser does not support the video tag.
          </video>
        </div>
      );
    }
    case FileTypeSupported.Audio: {
      return (
        <div className="flex h-full w-full items-center justify-center">
          <audio className="w-full" controls src={url}>
            Your browser does not support the audio tag.
          </audio>
        </div>
      );
    }
    case FileTypeSupported.Csv: {
      return <CsvPreview blob={blob} content={content} name={name} />;
    }
    case FileTypeSupported.Html: {
      return (
        <HtmlPreview blob={blob} content={content} name={name} url={url} />
      );
    }
    case FileTypeSupported.SqliteDatabase: {
      return <SqlitePreview url={url || ''} />;
    }
    default:
      return (
        <div className="text-text-secondary flex h-full flex-col items-center justify-center gap-6">
          <span>Preview not available for this file type</span>
        </div>
      );
  }
};

const FullscreenDialog = ({
  open,
  name,
  type,
  url,
  content,
  blob,
  setOpen,
  onDownload,
  onOpenInSystem,
  isOpeningInSystem,
}: Pick<Attachment, 'name' | 'url' | 'content' | 'type' | 'blob'> & {
  open: boolean;
  setOpen: (open: boolean) => void;
  onDownload?: () => void;
  onOpenInSystem?: () => void | Promise<void>;
  isOpeningInSystem?: boolean;
}) => (
  <Dialog onOpenChange={setOpen} open={open}>
    <DialogContent className="flex size-full max-h-[99vh] max-w-[99vw] flex-col gap-2 bg-transparent p-1 py-8">
      <div className="flex w-full items-center justify-between gap-16 px-10">
        <div className="text-text-default max-w-3xl truncate text-left text-sm">
          {name}
        </div>
        <div className="flex items-center gap-4">
          {onOpenInSystem && (
            <Button
              disabled={isOpeningInSystem}
              isLoading={isOpeningInSystem}
              onClick={() => {
                void onOpenInSystem();
              }}
              size="xs"
              variant="outline"
            >
              <ExternalLinkIcon className="size-4" />
              Open in default app
            </Button>
          )}
          <Button onClick={onDownload} size="xs" variant="outline">
            <DownloadIcon className="size-4" />
            Download
          </Button>
          <DialogClose>
            <XIcon className="text-text-secondary h-6 w-6" />
            <span className="sr-only">Close</span>
          </DialogClose>
        </div>
      </div>
      <div className="text-text-default flex size-full flex-col overflow-hidden rounded-l-xl p-10">
        <FileContentViewer
          content={content}
          blob={blob}
          name={name}
          type={type}
          url={url}
        />
      </div>
    </DialogContent>
  </Dialog>
);
const FileButton = ({
  name,
  size,
  onFullscreen,
}: Pick<Attachment, 'name' | 'size'> & {
  onFullscreen: (open: boolean) => void;
}) => (
  <button
    className="border-divider hover:bg-bg-secondary flex h-14 w-full max-w-[210px] min-w-[210px] shrink-0 cursor-pointer items-center gap-2 rounded-md border py-1.5 pr-1.5 pl-2 text-left"
    onClick={() => onFullscreen(true)}
  >
    <span className="bg-bg-quaternary text-text-default flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-xs transition-colors">
      {fileIconMap[getFileExt(name)] ? (
        <FileTypeIcon
          className="text-text-secondary h-5 w-5"
          type={getFileExt(name)}
        />
      ) : (
        <PaperClipIcon className="text-text-secondary h-4 w-4" />
      )}
    </span>
    <FileInfo fileName={name} fileSize={size} />
  </button>
);

export const FilePreview = ({
  name,
  url,
  size,
  content,
  blob,
  type,
  path,
}: Attachment) => {
  const [open, setOpen] = useState(false);
  const [isOpeningInSystem, setIsOpeningInSystem] = useState(false);
  const resolvedRelativePath = useMemo(
    () => resolveNodeStorageRelativePath(path),
    [path],
  );

  const fileName = decodeURIComponent(name).split('/').at(-1) ?? '';

  const children = isImageFile(name) ? (
    <ImagePreview name={name} onFullscreen={setOpen} size={size} url={url} />
  ) : (
    <FileButton name={name} onFullscreen={setOpen} size={size} />
  );

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div>{children}</div>
      </TooltipTrigger>
      <TooltipPortal>
        <TooltipContent className="container break-words" side="top">
          <p>{fileName}</p>
        </TooltipContent>
      </TooltipPortal>
      <FullscreenDialog
        content={content}
        blob={blob}
        name={fileName}
        onDownload={async () => {
          const currentFile =
            blob ??
            new Blob([content || url || ''], {
              type: 'application/octet-stream',
            });
          const arrayBuffer = await currentFile.arrayBuffer();
          const currentContent = new Uint8Array(arrayBuffer);
          const savePath = await save({
            defaultPath: fileName,
            filters: [
              {
                name: 'File',
                extensions: [getFileExt(name)],
              },
            ],
          });
          if (!savePath) {
            toast.info('File saving cancelled');
            return;
          }

          await fs.writeFile(savePath, currentContent, {
            baseDir: BaseDirectory.Download,
          });

          toast.success(`${fileName} downloaded successfully`);
        }}
        onOpenInSystem={
          resolvedRelativePath
            ? async () => {
                try {
                  console.log('resolvedRelativePath', resolvedRelativePath);
                  setIsOpeningInSystem(true);
                  await invoke('shinkai_node_open_storage_location_with_path', {
                    relativePath: resolvedRelativePath,
                  });
                } catch (error) {
                  console.error('Failed to open file in system:', error);
                  toast.error('Failed to open file in system');
                } finally {
                  setIsOpeningInSystem(false);
                }
              }
            : undefined
        }
        isOpeningInSystem={isOpeningInSystem}
        open={open}
        setOpen={setOpen}
        type={type}
        url={url}
      />
    </Tooltip>
  );
};

const animations = {
  initial: { scale: 0.97, opacity: 0, y: 10 },
  animate: {
    scale: 1,
    opacity: 1,
    y: 0,
    transition: {
      type: 'spring',
      stiffness: 200,
      damping: 20,
      mass: 0.8,
      velocity: 1,
      duration: 0.6,
    },
  },
  exit: {
    scale: 0.97,
    opacity: 0,
    y: -10,
    transition: {
      type: 'spring',
      stiffness: 150,
      damping: 15,
      duration: 0.4,
    },
  },
};

export const FileList = ({ files, className }: FileListProps) => {
  return (
    <ul className={cn('flex flex-wrap gap-3', className)}>
      <AnimatePresence>
        {files?.map((file, index) => (
          <motion.li {...animations} key={index}>
            <FilePreview {...file} />
          </motion.li>
        ))}
      </AnimatePresence>
    </ul>
  );
};
