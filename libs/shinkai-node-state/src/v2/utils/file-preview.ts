import { FileTypeSupported } from '../queries/getChatConversation/types';

export interface FilePreviewInfo {
  id: string;
  name: string;
  extension: string;
  path: string;
  size: number;
  type: FileTypeSupported;
  mimeType: string;
  url?: string;
  content?: string;
  blob?: Blob;
  error?: string;
}

export interface FilePreviewError {
  id: string;
  name: string;
  extension: string;
  path: string;
  size: number;
  type: FileTypeSupported.Error;
  mimeType: string;
  error: string;
}

const createErrorPreview = (
  file: string,
  error: unknown,
  context = '',
): FilePreviewError => {
  const errorMessage = error instanceof Error ? error.message : 'Unknown error';
  const contextMessage = context ? ` - ${context}` : '';

  return {
    id: 'error',
    name: file.split('/').at(-1) ?? 'unknown',
    extension: 'txt',
    path: file,
    size: 0,
    type: FileTypeSupported.Error,
    mimeType: 'text/plain',
    error: `${errorMessage}${contextMessage}`,
  };
};

export const generateFilePreview = async (
  file: string,
  data: Blob,
): Promise<FilePreviewInfo | FilePreviewError> => {
  if (!data) {
    return createErrorPreview(file, new Error('No data provided'));
  }

  try {
    const fileNameBase = file.split('/')?.at(-1) ?? 'untitled';
    const fileExtension = fileNameBase.split('.')?.at(-1) ?? '';

    const baseFileInfo: FilePreviewInfo = {
      id: file,
      name: fileNameBase,
      extension: fileExtension,
      path: file,
      size: data.size,
      blob: data,
      type: FileTypeSupported.Unknown,
      mimeType: 'application/octet-stream',
    };

    // Image files
    if (file.match(/\.(jpg|jpeg|png|gif)$/i)) {
      return {
        ...baseFileInfo,
        type: FileTypeSupported.Image,
        mimeType: `image/${fileExtension}`,
        url: URL.createObjectURL(data),
      };
    }

    // Video files
    if (file.match(/\.(mp4|webm|mov)$/i)) {
      return {
        ...baseFileInfo,
        type: FileTypeSupported.Video,
        mimeType: `video/${fileExtension}`,
        url: URL.createObjectURL(data),
      };
    }

    // Audio files
    if (file.match(/\.(mp3|wav|aac)$/i)) {
      return {
        ...baseFileInfo,
        type: FileTypeSupported.Audio,
        mimeType: `audio/${fileExtension}`,
        url: URL.createObjectURL(data),
      };
    }

    if (file.match(/\.(csv)$/i)) {
      const textContent = await data.text();
      return {
        ...baseFileInfo,
        type: FileTypeSupported.Csv,
        mimeType: 'text/csv',
        content: textContent,
      };
    }

    // Text files
    if (file.match(/\.(md|markdown|txt|log|tsx|json|js|jsx)$/i)) {
      const textContent = await data.text();
      return {
        ...baseFileInfo,
        type: FileTypeSupported.Text,
        mimeType: 'text/plain',
        content: textContent,
      };
    }

    // HTML files
    if (file.match(/\.(htm|html)$/i)) {
      return {
        ...baseFileInfo,
        type: FileTypeSupported.Html,
        mimeType: 'text/html',
        url: URL.createObjectURL(data),
      };
    }

    if (file.match(/\.(sqlite)$/i)) {
      return {
        ...baseFileInfo,
        type: FileTypeSupported.SqliteDatabase,
        mimeType: 'application/octet-stream',
        url: URL.createObjectURL(data),
      };
    }

    return baseFileInfo;
  } catch (error) {
    console.error(`Failed to generate preview for ${file}:`, error);
    return createErrorPreview(file, error, 'Failed to process file');
  }
};
