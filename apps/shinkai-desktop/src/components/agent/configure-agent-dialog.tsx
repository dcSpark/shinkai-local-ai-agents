import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type ToolOffering,
  type ToolUsageType,
} from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { useSetToolOffering } from '@shinkai_network/shinkai-node-state/v2/mutations/setToolOffering/useSetToolOffering';
import { type FormattedNetworkAgent } from '@shinkai_network/shinkai-node-state/v2/queries/getNetworkAgents/types';
import { useGetWalletList } from '@shinkai_network/shinkai-node-state/v2/queries/getWalletList/useGetWalletList';
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Button,
  Input,
  Textarea,
  Card,
  RadioGroup,
  Label,
  RadioGroupItem,
} from '@shinkai_network/shinkai-ui';

import { AnimatePresence, motion } from 'framer-motion';
import { Settings } from 'lucide-react';
import { useState, useEffect } from 'react';
import { toast } from 'sonner';
import { useAuth } from '../../store/auth';

interface ConfigureAgentDialogProps {
  agent: FormattedNetworkAgent;
}

export default function ConfigureAgentDialog({
  agent,
}: ConfigureAgentDialogProps) {
  const [open, setOpen] = useState(false);
  const [pricingType, setPricingType] = useState<'free' | 'paid'>('free');
  const [amount, setAmount] = useState('');
  const [description, setDescription] = useState('');

  const auth = useAuth((s) => s.auth);
  const { t } = useTranslation();

  const { data: walletInfo } = useGetWalletList({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const currentOffering = agent.apiData.tool_offering;

  const { mutateAsync: updateOffering, isPending } = useSetToolOffering({
    onSuccess: () => {
      setOpen(false);
      toast.success('Agent updated successfully');
    },
    onError: (error) => {
      toast.error('Failed to update agent');
    },
  });

  const formatUSDCAmount = (rawAmount: string): string => {
    if (!rawAmount || isNaN(Number(rawAmount))) return '0.000000';
    const usdcAmount = Number(rawAmount) / 1000000;
    return usdcAmount.toFixed(6);
  };

  useEffect(() => {
    if (currentOffering) {
      setDescription(currentOffering.meta_description || '');
      const perUse = currentOffering.usage_type.PerUse;
      if (
        typeof perUse === 'object' &&
        'Payment' in perUse &&
        perUse.Payment.length > 0 &&
        perUse.Payment[0].maxAmountRequired !== ''
      ) {
        setPricingType('paid');
        setAmount(perUse.Payment[0].maxAmountRequired || '');
      } else {
        setPricingType('free');
      }
    }
  }, [currentOffering]);

  const handleUpdate = async () => {
    if (
      !currentOffering ||
      !walletInfo?.payment_wallet?.data?.address?.address_id
    ) {
      toast.warning('Please connect your wallet to update the agent');
      return;
    }

    const usage: ToolUsageType =
      pricingType === 'free'
        ? { PerUse: 'Free' }
        : {
            PerUse: {
              Payment: [
                {
                  scheme: 'exact',
                  mimeType: 'application/json',
                  asset: 'USDC',
                  outputSchema: {},
                  resource: 'https://shinkai.com',
                  extra: { name: 'USDC', version: '1' },
                  payTo: walletInfo.payment_wallet.data.address.address_id,
                  description: description,
                  maxTimeoutSeconds: 300,
                  network: 'base-sepolia',
                  maxAmountRequired: amount,
                },
              ],
            },
          };

    const offering: ToolOffering = {
      meta_description: description,
      tool_key: currentOffering.tool_key,
      usage_type: usage,
    };

    await updateOffering({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      offering,
    });
  };

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open) {
          if (
            currentOffering.usage_type &&
            'PerUse' in currentOffering.usage_type
          ) {
            const perUse = currentOffering.usage_type.PerUse;
            if (perUse === 'Free') {
              setPricingType('free');
            } else if (
              typeof perUse === 'object' &&
              'Payment' in perUse &&
              perUse.Payment.length > 0
            ) {
              setPricingType('paid');
              setAmount(perUse.Payment[0].maxAmountRequired || '');
            } else {
              setPricingType('free');
            }
          } else {
            setPricingType('free');
          }
          setDescription(currentOffering?.meta_description || '');
        }
        setOpen(open);
      }}
      open={open}
    >
      <DialogTrigger asChild>
        <Button variant="outline" size="md">
          <Settings className="h-4 w-4" />
          {t('common.configure')}
        </Button>
      </DialogTrigger>
      <DialogContent showCloseButton className="max-w-[600px]">
        <DialogHeader>
          <DialogTitle>Configure Agent: {agent.name}</DialogTitle>
        </DialogHeader>

        <div className="flex max-h-[500px] flex-col gap-5 overflow-y-scroll">
          <div className="space-y-4">
            <label className="mb-0 block text-sm font-medium text-white">
              Pricing Model
            </label>
            <p className="text-official-gray-400 mt-1 mb-3 text-xs">
              Users will be charged this amount each time they use your agent.
            </p>
            <RadioGroup
              value={pricingType}
              onValueChange={(value: 'free' | 'paid') => setPricingType(value)}
              className="px-1"
            >
              <div className="space-y-3">
                <div className="border-official-gray-780 flex items-center gap-0 rounded-lg border px-4">
                  <RadioGroupItem value="free" id="pricing-free" />
                  <Label
                    htmlFor="pricing-free"
                    className="w-full px-4 py-3 font-medium"
                  >
                    <div className="flex items-center justify-between">
                      <div>
                        <p className="font-medium">Free</p>
                        <p className="text-official-gray-400 text-sm">
                          Free to use your agent
                        </p>
                      </div>
                    </div>
                  </Label>
                </div>

                <div className="border-official-gray-780 flex items-center gap-0 rounded-lg border px-4">
                  <RadioGroupItem value="paid" id="pricing-paid" />
                  <Label
                    htmlFor="pricing-paid"
                    className="w-full px-4 py-3 font-medium"
                  >
                    <div className="flex items-center justify-between">
                      <div>
                        <p className="font-medium">Paid (USDC)</p>
                        <p className="text-official-gray-400 text-sm">
                          Monetize your agent
                        </p>
                      </div>
                    </div>
                  </Label>
                </div>
              </div>
            </RadioGroup>

            <AnimatePresence initial={false}>
              {pricingType === 'paid' && (
                <motion.div
                  initial={{ opacity: 0, height: 0, y: -10 }}
                  animate={{ opacity: 1, height: 'auto', y: 0 }}
                  exit={{ opacity: 0, height: 0, y: -10 }}
                  transition={{ duration: 0.3, ease: 'easeInOut' }}
                >
                  <Card className="bg-official-gray-950 -mt-3 border-none px-5 py-2">
                    <Label htmlFor="price" className="text-sm font-medium">
                      Price per use (USDC units)
                    </Label>
                    <div className="mt-2 flex items-center gap-2">
                      <Input
                        placeholder="1.00"
                        value={amount}
                        className="!h-full py-2"
                        onChange={(e) => setAmount(e.target.value)}
                      />
                    </div>
                    <p className="text-official-gray-400 mt-1 text-xs">
                      = {formatUSDCAmount(amount)} USDC per use.
                    </p>
                  </Card>
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          <div>
            <label className="mb-2 block text-sm font-medium text-white">
              Agent description
            </label>
            <Textarea
              placeholder={t('agents.publishDialog.description')}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              resize="vertical"
              className="!min-h-[100px] pt-3"
            />
            <p className="text-official-gray-400 mt-1 text-xs">
              Help users understand what your agent does.
            </p>
          </div>
        </div>

        <DialogFooter className="mt-4 flex justify-end gap-2">
          <Button
            variant="outline"
            size="md"
            className="min-w-[100px]"
            onClick={() => setOpen(false)}
          >
            {t('common.cancel')}
          </Button>
          <Button
            onClick={handleUpdate}
            isLoading={isPending}
            className="min-w-[100px]"
            size="md"
          >
            {t('common.update')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
