import { DialogClose } from '@radix-ui/react-dialog';
import { Slot } from '@radix-ui/react-slot';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { FunctionKeyV2 } from '@shinkai_network/shinkai-node-state/v2/constants';
import { useRemoveTool } from '@shinkai_network/shinkai-node-state/v2/mutations/removeTool/useRemoveTool';
import {
  Button,
  buttonVariants,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
  DialogTrigger,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { useQueryClient } from '@tanstack/react-query';
import { Trash2 } from 'lucide-react';
import { useState } from 'react';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';

export default function RemoveToolButton({ toolKey }: { toolKey: string }) {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();
  const [isOpen, setIsOpen] = useState(false);
  const queryClient = useQueryClient();
  console.log('toolKey', toolKey);
  const { mutateAsync: removeTool, isPending: isRemoveToolPending } =
    useRemoveTool({
      onSuccess: async () => {
        await queryClient.invalidateQueries({
          queryKey: [FunctionKeyV2.GET_LIST_TOOLS],
        });
        await queryClient.invalidateQueries({
          queryKey: [FunctionKeyV2.GET_SEARCH_TOOLS],
        });

        toast.success('Tool has been removed successfully');
        setIsOpen(false);
        await navigate('/tools');
      },
      onError: (error) => {
        toast.error('Failed to remove tool', {
          description: error.response?.data?.message ?? error.message,
        });
      },
    });

  const handleRemove = async () => {
    await removeTool({
      toolKey: toolKey ?? '',
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    });
  };

  return (
    <Dialog onOpenChange={setIsOpen} open={isOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DialogTrigger asChild>
            <button
              className={cn(
                buttonVariants({
                  variant: 'outline',
                  size: 'sm',
                }),
                'flex h-auto min-h-auto w-10 justify-center rounded-md py-2',
              )}
            >
              <Trash2 className="h-4 w-4" />
            </button>
          </DialogTrigger>
        </TooltipTrigger>
        <TooltipContent align="center" side="top">
          {t('common.deleteTool', 'Delete Tool')}
        </TooltipContent>
      </Tooltip>
      <DialogContent className="sm:max-w-[425px]">
        <DialogTitle className="pb-0">Delete Tool</DialogTitle>
        <DialogDescription>
          Are you sure you want to delete this tool? This action cannot be
          undone.
        </DialogDescription>

        <DialogFooter>
          <div className="flex gap-2 pt-4">
            <DialogClose asChild className="flex-1">
              <Button
                className="min-w-[100px] flex-1"
                size="sm"
                type="button"
                variant="ghost"
              >
                {t('common.cancel')}
              </Button>
            </DialogClose>
            <Button
              className="min-w-[100px] flex-1"
              isLoading={isRemoveToolPending}
              onClick={handleRemove}
              size="sm"
              variant="destructive"
            >
              {t('common.delete')}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
