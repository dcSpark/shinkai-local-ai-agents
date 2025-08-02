import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type Agent } from '@shinkai_network/shinkai-message-ts/api/agents/types';
import {
  type ToolOffering,
  type ToolUsageType,
} from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { useSetToolOffering } from '@shinkai_network/shinkai-node-state/v2/mutations/setToolOffering/useSetToolOffering';
import { useGetAgents } from '@shinkai_network/shinkai-node-state/v2/queries/getAgents/useGetAgents';
import { useGetToolsWithOfferings } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsWithOfferings/useGetToolsWithOfferings';
import { useGetWalletList } from '@shinkai_network/shinkai-node-state/v2/queries/getWalletList/useGetWalletList';
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Button,
  SearchInput,
  Input,
  Textarea,
  Card,
  RadioGroup,
  Label,
  RadioGroupItem,
  Checkbox,
  Badge,
} from '@shinkai_network/shinkai-ui';
import { AIAgentIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { AnimatePresence, motion } from 'framer-motion';
import { ArrowRightIcon, PlusIcon } from 'lucide-react';
import { useState, useMemo, useEffect } from 'react';
import { toast } from 'sonner';
import { useAuth } from '../../store/auth';
type WizardStep = 'select' | 'configure' | 'publishing' | 'success';

export default function PublishAgentDialog() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [currentStep, setCurrentStep] = useState<WizardStep>('select');
  const [selected, setSelected] = useState<Agent | null>(null);
  const [pricingType, setPricingType] = useState<'free' | 'paid'>('free');
  const [payTo, setPayTo] = useState('');
  const [amount, setAmount] = useState('');
  const [description, setDescription] = useState('');
  const [acceptedTerms, setAcceptedTerms] = useState(false);

  const auth = useAuth((s) => s.auth);
  const { t } = useTranslation();

  const { data: walletInfo } = useGetWalletList({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { data: agents } = useGetAgents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { data: offerings } = useGetToolsWithOfferings({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const publishedKeys = useMemo(
    () => new Set((offerings ?? []).map((o) => o.tool_offering.tool_key)),
    [offerings],
  );

  const filteredAgents = useMemo(
    () =>
      (agents ?? []).filter((agent) =>
        agent.name.toLowerCase().includes(search.toLowerCase()),
      ),
    [agents, search],
  );

  const asset = 'USDC';

  const { mutateAsync: publish, isPending } = useSetToolOffering({
    onSuccess: () => {
      setSelected(null);
      setPayTo('');
      setAmount('');
      setDescription('');
      setOpen(false);
    },
  });

  const formatUSDCAmount = (rawAmount: string): string => {
    if (!rawAmount || isNaN(Number(rawAmount))) return '0.000000';
    const usdcAmount = Number(rawAmount) / 1000000;
    return usdcAmount.toFixed(6);
  };

  useEffect(() => {
    if (selected && walletInfo?.payment_wallet?.data?.address?.address_id) {
      setPayTo(walletInfo.payment_wallet.data.address.address_id);
    }
  }, [selected, walletInfo]);

  const handlePublish = async () => {
    if (!selected) return;
    const usage: ToolUsageType = {
      PerUse: {
        Payment: [
          {
            scheme: 'exact',
            mimeType: 'application/json',
            asset: asset,
            outputSchema: {},
            resource: 'https://shinkai.com',
            extra: { name: asset, version: '1' },
            payTo: payTo,
            description: description,
            maxTimeoutSeconds: 300,
            network: 'base-sepolia',
            maxAmountRequired: amount,
          },
        ],
      },
    };

    if (!selected.tools.length) {
      toast.warning('Please select an agent with tools to publish');
      return;
    }

    const offering: ToolOffering = {
      meta_description: description,
      tool_key: selected.tools[0],
      usage_type: usage,
    };

    await publish({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      offering,
    });
  };

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open) {
          setCurrentStep('select');
          setSelected(null);
          setPayTo('');
          setAmount('');
          setDescription('');
          setAcceptedTerms(false);
        }
        setOpen(open);
      }}
      open={open}
    >
      <DialogTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          disabled={!walletInfo?.payment_wallet?.data?.address?.address_id}
        >
          <PlusIcon className="h-4 w-4" />
          {t('agents.publishDialog.open')}
        </Button>
      </DialogTrigger>
      <DialogContent showCloseButton className="max-w-[600px]">
        <DialogHeader>
          <DialogTitle>{t('agents.publishDialog.open')}</DialogTitle>
        </DialogHeader>
        {(currentStep === 'select' || currentStep === 'configure') && (
          <div className="-mx-[24px]">
            <div className="bg-bg-tertiary border-divider my-2 w-full border-b py-4">
              <div className="mx-auto flex max-w-[400px] flex-col">
                <div className="flex w-full items-center px-2">
                  <div
                    className={cn(
                      'ml-[40px] flex items-center justify-center rounded-full text-sm font-semibold',
                      'z-10 h-7 w-7',
                      currentStep === 'select'
                        ? 'bg-brand text-white'
                        : currentStep === 'configure'
                          ? 'bg-brand'
                          : 'bg-bg-quaternary text-text-secondary',
                    )}
                  >
                    1
                  </div>
                  <div className="relative -mx-2 h-2 flex-1">
                    <div
                      className={cn(
                        'absolute top-1/2 right-0 left-0 h-4 -translate-y-1/2 rounded',
                        currentStep === 'configure'
                          ? 'bg-brand'
                          : 'bg-bg-quaternary',
                      )}
                    />
                  </div>
                  <div
                    className={cn(
                      'mr-[50px] flex items-center justify-center rounded-full text-sm font-semibold',
                      'z-10 h-7 w-7',
                      currentStep === 'configure'
                        ? 'bg-brand text-white'
                        : 'bg-bg-quaternary text-text-secondary',
                    )}
                  >
                    2
                  </div>
                </div>
                <div className="mt-2 flex w-full items-center px-2">
                  <div className="flex-1 pl-1 text-left">
                    <span
                      className={cn(
                        'text-sm font-medium',
                        currentStep === 'select'
                          ? 'text-text-default'
                          : 'text-text-secondary',
                      )}
                    >
                      Choose Agent
                    </span>
                  </div>
                  <div className="flex-1 pr-1 text-right">
                    <span
                      className={cn(
                        'text-sm font-medium',
                        currentStep === 'configure'
                          ? 'text-text-default'
                          : 'text-text-secondary',
                      )}
                    >
                      Configure & Publish
                    </span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}
        {currentStep === 'select' && (
          <>
            <SearchInput
              placeholder={t('agents.publishDialog.searchAgents')}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              classNames={{ input: 'bg-transparent' }}
            />
            <div className="max-h-[472px] min-h-[300px] overflow-y-auto px-2 py-2">
              {filteredAgents.length > 0 && (
                <RadioGroup
                  value={selected?.agent_id}
                  onValueChange={(value) =>
                    setSelected(
                      filteredAgents.find((a) => a.agent_id === value) ?? null,
                    )
                  }
                  className="gap-1.5"
                >
                  {filteredAgents?.map((agent) => (
                    <div
                      key={agent.agent_id}
                      className={cn(
                        'border-divider flex items-center gap-0 rounded-lg border px-4',
                        publishedKeys.has(agent.tools[0]) && 'hidden',
                      )}
                    >
                      <RadioGroupItem
                        value={agent.agent_id}
                        id={agent.agent_id}
                        disabled={publishedKeys.has(agent.tools[0])}
                      />
                      <Label htmlFor={agent.agent_id} className="font-medium">
                        <Card className="flex items-center gap-3 border-0 bg-transparent p-4 shadow-none">
                          <div className="flex size-8 items-center justify-center rounded-lg">
                            <AIAgentIcon name={agent.name} size="sm" />
                          </div>
                          <div>
                            <p className="flex items-center gap-2 text-sm font-medium">
                              {agent.name}{' '}
                              {agent.tools.length > 0 && (
                                <Badge
                                  variant="inputAdornment"
                                  className="text-text-secondary text-xs font-bold"
                                >
                                  {agent.tools.length
                                    ? `${agent.tools.length} tools`
                                    : 'No tools available'}
                                </Badge>
                              )}
                            </p>
                            <p className="text-text-secondary line-clamp-1 text-sm">
                              {agent.ui_description}
                            </p>
                          </div>
                        </Card>
                      </Label>
                    </div>
                  ))}
                </RadioGroup>
              )}
            </div>
            <DialogFooter className="mt-1 flex justify-end gap-2">
              <Button
                variant="outline"
                size="md"
                className="min-w-[100px]"
                onClick={() => setOpen(false)}
              >
                {t('common.cancel')}
              </Button>
              <Button
                className="min-w-[100px]"
                onClick={() => setCurrentStep('configure')}
                size="md"
                disabled={!selected}
              >
                {t('common.continue')}
                <ArrowRightIcon className="h-4 w-4" />
              </Button>
            </DialogFooter>
          </>
        )}
        {currentStep === 'configure' && (
          <>
            <div className="flex !max-h-[500px] flex-col gap-5 overflow-y-scroll">
              <div>
                <label className="mb-2 block text-sm font-medium text-white">
                  Selected Agent
                </label>
                <Input
                  className="!h-[40px] py-2"
                  value={selected?.name}
                  disabled
                  readOnly
                />
              </div>
              <div>
                <label className="mb-2 block text-sm font-medium text-white">
                  Payment Address
                </label>
                <Input
                  className="!h-[40px] py-2"
                  placeholder={t('agents.publishDialog.paymentAddress')}
                  value={payTo}
                  disabled
                  readOnly
                />
              </div>

              <div className="space-y-4">
                <label className="mb-0 block text-sm font-medium text-white">
                  Pricing Model
                </label>
                <p className="text-text-secondary mt-1 mb-3 text-xs">
                  Users will be charged this amount each time they use your
                  agent.
                </p>
                <RadioGroup
                  value={pricingType}
                  onValueChange={(value: 'free' | 'paid') =>
                    setPricingType(value)
                  }
                  className="px-1"
                >
                  <div className="space-y-3">
                    <div className="border-divider flex items-center gap-0 rounded-lg border px-4">
                      <RadioGroupItem
                        value="free"
                        id="pricing-free"
                        className="peer"
                      />
                      <Label
                        htmlFor="pricing-free"
                        className="w-full px-4 py-3 font-medium"
                      >
                        <div className="flex items-center justify-between">
                          <div>
                            <p className="font-medium">Free</p>
                            <p className="text-text-secondary text-sm">
                              Free to use your agent
                            </p>
                          </div>
                        </div>
                      </Label>
                    </div>

                    <div className="border-divider flex items-center gap-0 rounded-lg border px-4">
                      <RadioGroupItem
                        value="paid"
                        id="pricing-paid"
                        className="peer"
                      />
                      <Label
                        htmlFor="pricing-paid"
                        className="w-full px-4 py-3 font-medium"
                      >
                        <div className="flex items-center justify-between">
                          <div>
                            <p className="font-medium">Paid (USDC)</p>
                            <p className="text-text-secondary text-sm">
                              Monetize your agent
                            </p>
                          </div>
                        </div>
                      </Label>
                    </div>
                  </div>
                </RadioGroup>

                <AnimatePresence>
                  {pricingType === 'paid' && (
                    <motion.div
                      initial={{ opacity: 0, height: 0, y: -10 }}
                      animate={{ opacity: 1, height: 'auto', y: 0 }}
                      exit={{ opacity: 0, height: 0, y: -10 }}
                      transition={{ duration: 0.3, ease: 'easeInOut' }}
                    >
                      <Card className="bg-bg-dark -mt-3 border-none px-5 py-2">
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
                        <p className="text-text-secondary mt-1 text-xs">
                          ={' '}
                          {amount
                            ? formatUSDCAmount(amount)
                            : formatUSDCAmount('1.00')}{' '}
                          USDC per use.
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
                <p className="text-text-secondary mt-1 text-xs">
                  Help users understand what your agent does.
                </p>
              </div>
              <div className="space-y-3">
                <div className="flex items-start space-x-2">
                  <Checkbox
                    id="terms"
                    checked={acceptedTerms}
                    onCheckedChange={(checked) =>
                      setAcceptedTerms(checked as boolean)
                    }
                  />
                  <Label
                    htmlFor="terms"
                    className="cursor-pointer text-sm leading-relaxed"
                  >
                    I understand that Shinkai reserves the right to remove any
                    agent that violates our content policy.
                  </Label>
                </div>
              </div>
            </div>
            <DialogFooter className="mt-4 flex justify-end gap-2">
              <Button
                variant="outline"
                size="md"
                className="min-w-[100px]"
                onClick={() => setCurrentStep('select')}
              >
                {t('common.back')}
              </Button>
              <Button
                onClick={handlePublish}
                isLoading={isPending}
                className="min-w-[100px]"
                size="md"
              >
                {t('agents.publishDialog.publish')}
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
