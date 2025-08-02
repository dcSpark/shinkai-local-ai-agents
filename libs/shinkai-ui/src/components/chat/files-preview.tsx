import { DialogClose } from '@radix-ui/react-dialog';
import {
  type Attachment,
  FileTypeSupported,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { save } from '@tauri-apps/plugin-dialog';
import * as fs from '@tauri-apps/plugin-fs';
import { BaseDirectory } from '@tauri-apps/plugin-fs';
import { partial } from 'filesize';
import { AnimatePresence, motion } from 'framer-motion';
import { CircleSlashIcon, XIcon } from 'lucide-react';
import React, { useState } from 'react';
import { toast } from 'sonner';

import { fileIconMap, FileTypeIcon, PaperClipIcon } from '../../assets/icons';
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

type FileContentViewerProps = Pick<
  Attachment,
  'name' | 'url' | 'content' | 'type'
>;

export const FileContentViewer: React.FC<FileContentViewerProps> = ({
  name,
  content,
  url,
  type,
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
    case FileTypeSupported.Html: {
      return (
        <div className="flex h-full w-full items-center justify-center">
          <iframe
            className="h-full w-full bg-gray-100"
            sandbox="allow-same-origin"
            src={url}
            title={name}
          />
        </div>
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
  setOpen,
  onDownload,
}: Pick<Attachment, 'name' | 'url' | 'content' | 'type'> & {
  open: boolean;
  setOpen: (open: boolean) => void;
  onDownload?: () => void;
}) => (
  <Dialog onOpenChange={setOpen} open={open}>
    <DialogContent className="flex size-full max-h-[99vh] max-w-[99vw] flex-col gap-2 bg-transparent p-1 py-8">
      <div className="flex w-full items-center justify-between gap-16 px-10">
        <div className="text-text-default max-w-3xl truncate text-left text-sm">
          {name}
        </div>
        <div className="flex items-center gap-4">
          <Button onClick={onDownload} size="xs" variant="outline">
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
}: Attachment) => {
  const [open, setOpen] = useState(false);

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
