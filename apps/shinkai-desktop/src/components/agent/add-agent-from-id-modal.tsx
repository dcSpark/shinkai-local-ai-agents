import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Input,
} from '@shinkai_network/shinkai-ui';
import { useState, useEffect } from 'react';
import { useAuth } from '../../store/auth';
import { useGetAgentNetworkOffering } from '@shinkai_network/shinkai-node-state/v2/queries/getAgentNetworkOffering/useGetAgentNetworkOffering';

export default function AddAgentFromIdModal() {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const [open, setOpen] = useState(false);
  const [agentId, setAgentId] = useState('');

  const { refetch } = useGetAgentNetworkOffering(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      agentId,
    },
    { enabled: false },
  );

  useEffect(() => {
    if (open) {
      setAgentId('@@');
    }
  }, [open]);

  const handleAdd = async () => {
    if (!auth) return;
    const result = await refetch();
    console.log(result.data);
    setOpen(false);
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm">
          {t('networkAgentsPage.addAgentFromId')}
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('networkAgentsPage.addAgentFromId')}</DialogTitle>
          <DialogDescription className="space-y-2">
            <p>{t('networkAgentsPage.addAgentFromIdDescription')}</p>
          </DialogDescription>
        </DialogHeader>
        <Input
          placeholder={t('networkAgentsPage.addAgentFromIdPlaceholder')}
          value={agentId}
          onChange={(e) => setAgentId(e.target.value)}
        />
        <DialogFooter className="flex-row gap-1">
          <Button variant="outline" size="md" onClick={() => setOpen(false)}>
            {t('common.cancel')}
          </Button>
          <Button size="md" onClick={handleAdd}>
            {t('common.add')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
