import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { isShinkaiIdentityLocalhost } from '@shinkai_network/shinkai-message-ts/utils';
import { useUpdateNodeName } from '@shinkai_network/shinkai-node-state/v2/mutations/updateNodeName/useUpdateNodeName';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Button,
  Input,
  Label,
  DialogFooter,
  DialogTrigger,
  buttonVariants,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { Edit3Icon, ExternalLinkIcon } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { useShinkaiNodeRespawnMutation } from '../../lib/shinkai-node-manager/shinkai-node-manager-client';
import { isHostingShinkaiNode } from '../../lib/shinkai-node-manager/shinkai-node-manager-windows-utils';
import { type Auth, useAuth } from '../../store/auth';
import { useShinkaiNodeManager } from '../../store/shinkai-node-manager';

const ShinkaiIdentityDialog = () => {
  const [isOpen, setIsOpen] = useState(false);

  const [newIdentity, setNewIdentity] = useState('');
  const { t } = useTranslation();
  const auth = useAuth((authStore) => authStore.auth);
  const setAuth = useAuth((authStore) => authStore.setAuth);
  const isLocalShinkaiNodeInUse = useShinkaiNodeManager(
    (state) => state.isInUse,
  );
  const { mutateAsync: respawnShinkaiNode } = useShinkaiNodeRespawnMutation();

  const { mutateAsync: updateNodeName, isPending: isUpdateNodeNamePending } =
    useUpdateNodeName({
      onSuccess: async () => {
        toast.success(t('settings.shinkaiIdentity.success'));
        if (!auth) return;
        const newAuth: Auth = { ...auth };
        setAuth({
          ...newAuth,
          shinkai_identity: newIdentity,
        });
        if (isLocalShinkaiNodeInUse) {
          await respawnShinkaiNode();
        } else if (!isHostingShinkaiNode(auth.node_address)) {
          toast.info(t('shinkaiNode.restartNode'));
        }
      },
      onError: (error) => {
        toast.error(t('settings.shinkaiIdentity.error'), {
          description: error?.response?.data?.error
            ? error?.response?.data?.error +
              ': ' +
              error?.response?.data?.message
            : error.message,
        });
      },
    });

  const handleCancel = () => {
    setNewIdentity('');
    setIsOpen(false);
  };

  const handleUpdateNodeName = async () => {
    if (!auth) return;
    await updateNodeName({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      newNodeName: newIdentity,
    });
  };

  const isIdentityLocalhost = isShinkaiIdentityLocalhost(
    auth?.shinkai_identity ?? '',
  );

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setIsOpen(true)}
          disabled={isUpdateNodeNamePending}
          className="min-w-[100px] gap-2 rounded-md"
        >
          <span className="sr-only"> Update Shinkai Identity</span>
          <span className="text-text-default text-sm font-normal">
            {auth?.shinkai_identity}
          </span>
          <Edit3Icon className="h-4 w-4" />
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-xl" showCloseButton>
        <DialogHeader>
          <DialogTitle className="font-semibold">
            Update Shinkai Identity
          </DialogTitle>
          <DialogDescription className="text-text-secondary text-sm">
            Connect your identity so you can take part in the Shinkai Network
            and use p2p capabilities
          </DialogDescription>
        </DialogHeader>

        <div className="mt-6 space-y-4">
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between gap-1">
              <Label htmlFor="identity">Enter the new Shinkai Identity</Label>
              <div className="text-text-secondary hover:text-text-default inline-flex items-center gap-1">
                {isIdentityLocalhost ? (
                  <a
                    className={cn(
                      buttonVariants({
                        size: 'auto',
                        variant: 'link',
                      }),
                      'rounded-lg p-0 text-xs text-inherit hover:underline',
                    )}
                    href={`https://shinkai-contracts.pages.dev?encryption_pk=${auth?.encryption_pk}&signature_pk=${auth?.identity_pk}&node_address=${auth?.node_address}`}
                    rel="noreferrer"
                    target="_blank"
                  >
                    {t('settings.shinkaiIdentity.registerIdentity')}
                  </a>
                ) : (
                  <a
                    className={cn(
                      buttonVariants({
                        size: 'auto',
                        variant: 'link',
                      }),
                      'rounded-lg p-0 text-xs text-inherit hover:underline',
                    )}
                    href={`https://shinkai-contracts.pages.dev/identity/${auth?.shinkai_identity?.replace(
                      '@@',
                      '',
                    )}`}
                    rel="noreferrer"
                    target="_blank"
                  >
                    {t('settings.shinkaiIdentity.goToShinkaiIdentity')}
                  </a>
                )}
                <ExternalLinkIcon className="h-4 w-4" />
              </div>
            </div>
            <Input
              id="identity"
              value={newIdentity}
              onChange={(e) => setNewIdentity(e.target.value)}
              placeholder={`${auth?.shinkai_identity}`}
              disabled={isUpdateNodeNamePending}
              className="!h-10 py-2"
            />
          </div>
        </div>

        <DialogFooter className="mt-4 flex items-center !justify-between gap-1">
          <a
            className={cn(
              buttonVariants({
                size: 'auto',
                variant: 'link',
              }),
              'text-text-secondary hover:text-text-default block rounded-lg p-0 text-left text-xs hover:underline',
            )}
            href="https://docs.shinkai.com/advanced/shinkai-identity"
            rel="noreferrer"
            target="_blank"
          >
            {t('settings.shinkaiIdentity.troubleRegisterIdentity')}
          </a>
          <div className="flex items-center gap-1">
            <Button
              onClick={handleCancel}
              disabled={isUpdateNodeNamePending}
              variant="outline"
              className="w-auto min-w-[110px]"
              size="md"
            >
              Cancel
            </Button>
            <Button
              onClick={handleUpdateNodeName}
              disabled={
                !newIdentity.trim() ||
                newIdentity === auth?.shinkai_identity ||
                isUpdateNodeNamePending
              }
              className="w-auto min-w-[110px]"
              size="md"
            >
              {isUpdateNodeNamePending ? 'Updating...' : 'Update Identity'}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default ShinkaiIdentityDialog;
