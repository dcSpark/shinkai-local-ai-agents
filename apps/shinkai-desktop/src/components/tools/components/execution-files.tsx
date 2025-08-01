import { useGetShinkaiFilesProtocol } from '@shinkai_network/shinkai-node-state/v2/queries/getShinkaiFileProtocol/useGetShinkaiFilesProtocol';
import { FileList } from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';

import { useAuth } from '../../../store/auth';

type ExecutionFilesProps = {
  files: string[];
  className?: string;
};

export function ExecutionFiles({ files, className }: ExecutionFilesProps) {
  const auth = useAuth((state) => state.auth);
  const { data } = useGetShinkaiFilesProtocol(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      files,
    },
    {
      enabled: !!files && files.length > 0,
    },
  );

  if (!files || files.length === 0 || !data || data.length === 0) {
    return null;
  }

  return (
    <div className={cn('mt-4 flex flex-col items-start gap-1', className)}>
      <span className="text-text-secondary text-xs">Generated Files</span>
      <div className="mt-2 flex w-full flex-wrap gap-2">
        <FileList
          className="mt-2"
          files={data?.map((file) => ({
            name: file.name,
            path: file.path,
            type: file.type,
            size: file?.size,
            content: file?.content,
            blob: file?.blob,
            id: file.id,
            extension: file.extension,
            mimeType: file.mimeType,
            url: file.url,
          }))}
        />
      </div>
    </div>
  );
}
