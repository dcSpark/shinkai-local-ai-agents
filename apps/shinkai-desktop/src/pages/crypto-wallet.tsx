import { zodResolver } from '@hookform/resolvers/zod';
import { DialogClose } from '@radix-ui/react-dialog';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  NetworkIdentifier,
  WalletRole,
} from '@shinkai_network/shinkai-message-ts/api/wallets';
import { useCreateLocalWallet } from '@shinkai_network/shinkai-node-state/v2/mutations/createLocalWallet/useCreateLocalWallet';
import { useRestoreCoinbaseMpcWallet } from '@shinkai_network/shinkai-node-state/v2/mutations/restoreCoinbaseMpcWallet/useRestoreCoinbaseMpcWallet';
import { useRestoreLocalWallet } from '@shinkai_network/shinkai-node-state/v2/mutations/restoreLocalWallet/useRestoreLocalWallet';
import { useGetWalletBalance } from '@shinkai_network/shinkai-node-state/v2/queries/getWalletBalance/useGetWalletBalance';
import { useGetWalletList } from '@shinkai_network/shinkai-node-state/v2/queries/getWalletList/useGetWalletList';
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  buttonVariants,
  Card,
  CardContent,
  CopyToClipboardIcon,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Separator,
  Switch,
  Textarea,
  TextField,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import {
  AddCryptoWalletIcon,
  EthereumIcon,
  USDCIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';
import {
  useCopyClipboard,
  useMeasure,
} from '@shinkai_network/shinkai-ui/hooks';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { AnimatePresence, motion } from 'framer-motion';
import {
  ArrowLeft,
  Copy,
  Download,
  EyeIcon,
  EyeOffIcon,
  FileText,
  PlusIcon,
  RefreshCw,
  XIcon,
  ExternalLinkIcon,
  CheckCircle2,
  AlertTriangle,
  MoreVertical,
} from 'lucide-react';
import { useMemo, useRef, useState } from 'react';
import { useForm } from 'react-hook-form';
import { Link } from 'react-router';
import { toast } from 'sonner';
import { z } from 'zod';

import {
  useWalletsStore,
  WalletCreateConnectView,
} from '../components/crypto-wallet/context/wallets-context';
import { formatBalanceAmount } from '../components/crypto-wallet/utils';
import { useAuth } from '../store/auth';
import { SimpleLayout } from './layout/simple-layout';

const SecretRecoveryPhraseDisplay = ({
  mnemonic,
  showTitle = true,
  showWarning = true,
}: {
  mnemonic: string;
  showTitle?: boolean;
  showWarning?: boolean;
}) => {
  const { isCopied, onCopy } = useCopyClipboard({
    string: mnemonic,
  });

  return (
    <div>
      {showTitle && (
        <DialogHeader className="mb-5">
          <DialogTitle className="text-center">
            Secret Recovery Phrase
          </DialogTitle>
        </DialogHeader>
      )}

      {showWarning && (
        <Alert variant="warning" className="mb-3 bg-amber-500/10">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle className="mb-2 text-base font-medium">
            Never share your Secret Recovery Phrase
          </AlertTitle>
          <AlertDescription className="!pl-5 text-sm text-white">
            <ul className="list-disc space-y-1 text-sm">
              <li>
                Anyone with access to your Secret Recovery Phrase can take full
                control of your wallet and funds.
              </li>
              <li>
                Do not share it with anyone — even if they claim to be support.
              </li>
              <li>Store it offline in a secure, private place.</li>
              <li>
                Revealing your phrase is for backup purposes only. Proceed with
                caution.
              </li>
            </ul>
          </AlertDescription>
        </Alert>
      )}

      <div className="space-y-2">
        <div className="text-text-secondary mb-2 text-xs">
          Hover over each word to reveal
        </div>
        <div className="mb-4 grid grid-cols-2 gap-x-8 gap-y-1 rounded-lg border bg-white/5 p-4">
          {mnemonic?.split(' ').map((word, idx) => (
            <div
              key={idx}
              className="group flex cursor-pointer items-center gap-3"
            >
              <span className="flex h-6 w-6 min-w-[24px] shrink-0 items-center justify-center rounded-full bg-gray-700 text-sm font-medium text-white">
                {idx + 1}
              </span>
              <span className="group relative rounded-md bg-gray-800/40 px-2 py-1 transition-colors duration-200 hover:bg-gray-700/30">
                <span
                  className="font-mono text-sm [filter:blur(4px)] transition-all duration-300 ease-out select-none group-hover:[filter:blur(0px)]"
                  style={{
                    textShadow: '0 0 8px rgba(255,255,255,0.8)',
                  }}
                >
                  {word}
                </span>
              </span>
            </div>
          ))}
        </div>
      </div>

      <DialogFooter className="flex items-center justify-between gap-2 md:flex-col">
        <Button variant="outline" size="md" className="w-full" onClick={onCopy}>
          {isCopied ? (
            <CheckCircle2 className="size-4" />
          ) : (
            <Copy className="size-4" />
          )}
          {isCopied ? 'Copied' : 'Copy to Clipboard'}
        </Button>
      </DialogFooter>
    </div>
  );
};

const CryptoWalletPage = () => {
  const { t } = useTranslation();
  const [showRecoveryPhraseModal, setShowRecoveryPhraseModal] = useState(false);

  const auth = useAuth((state) => state.auth);

  const { data: walletInfo } = useGetWalletList({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const walletExist =
    walletInfo?.payment_wallet || walletInfo?.receiving_wallet;

  const {
    data: walletBalance,
    refetch,
    isRefetching,
  } = useGetWalletBalance(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    },
    { enabled: !!walletExist },
  );

  const etherscanLink = useMemo(() => {
    const address = walletInfo?.payment_wallet?.data?.address?.address_id;
    const network = walletInfo?.payment_wallet?.data?.network;
    if (!address || !network) return '';
    const baseUrl =
      network === NetworkIdentifier.BaseSepolia
        ? 'https://sepolia.basescan.org'
        : 'https://basescan.org';
    return `${baseUrl}/address/${address}`;
  }, [
    walletInfo?.payment_wallet?.data?.address?.address_id,
    walletInfo?.payment_wallet?.data?.network,
  ]);

  return (
    <SimpleLayout
      classname="container"
      headerRightElement={
        walletExist ? (
          <div className="flex items-center gap-3">
            <Link
              to="/network-ai-agents"
              className={cn(
                buttonVariants({
                  variant: 'outline',
                  size: 'sm',
                }),
              )}
            >
              Explore Network Agents
            </Link>
            <CreateWalletDialog buttonLabel="Update Wallet" />
          </div>
        ) : null
      }
      title={walletExist ? 'My Wallet' : t('settings.cryptoWallet.title')}
    >
      {walletExist ? (
        <>
          <div className="min-h-screen py-5">
            <div className="mx-auto space-y-6">
              <Card>
                <CardContent className="space-y-4 pt-5">
                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <div className="text-text-secondary text-sm font-medium">
                        Wallet Address
                      </div>
                      {walletInfo?.payment_wallet?.data?.mnemonic && (
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="outline" size="icon">
                              <MoreVertical className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end" className="p-2">
                            <DropdownMenuItem
                              onClick={() => setShowRecoveryPhraseModal(true)}
                            >
                              Reveal Secret Recovery Phrase
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      )}
                    </div>
                    <div className="flex items-center gap-3">
                      <code className="font-mono text-base">
                        {walletInfo?.payment_wallet?.data?.address?.address_id}
                      </code>
                      <CopyToClipboardIcon
                        string={
                          walletInfo?.payment_wallet?.data?.address?.address_id
                        }
                        className="size-4"
                      />
                      {etherscanLink && (
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <a
                              href={etherscanLink}
                              target="_blank"
                              className={cn(
                                buttonVariants({
                                  variant: 'tertiary',
                                  size: 'icon',
                                }),
                              )}
                            >
                              <ExternalLinkIcon className="h-3.5 w-3.5" />
                            </a>
                          </TooltipTrigger>
                          <TooltipContent>
                            <p>View on Explorer</p>
                          </TooltipContent>
                        </Tooltip>
                      )}
                    </div>
                  </div>
                  <Separator />
                  <div className="space-y-2">
                    <div className="text-text-secondary text-sm font-medium">
                      Network
                    </div>
                    <p className="text-base text-white">
                      {formatText(walletInfo?.payment_wallet?.data?.network)}
                    </p>
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardContent className="space-y-6 pt-5">
                  <div className="flex items-center justify-between">
                    <div className="text-base font-medium">Assets</div>
                    <Button
                      className="h-8 w-auto"
                      disabled={isRefetching}
                      isLoading={isRefetching}
                      onClick={() => refetch()}
                      rounded="lg"
                      size="xs"
                      variant="outline"
                    >
                      {!isRefetching && <RefreshCw className="h-4 w-4" />}
                    </Button>
                  </div>
                  <div className="space-y-4">
                    {/* ETH */}
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <div className="flex size-9 items-center justify-center rounded-full border bg-black">
                          <EthereumIcon className="size-5" />
                        </div>
                        <div>
                          <div className="font-medium">Ethereum</div>
                          <div className="text-text-secondary text-sm">ETH</div>
                        </div>
                      </div>
                      <div className="text-right">
                        <div className="font-medium">
                          {formatBalanceAmount(
                            walletBalance?.ETH.amount ?? '0',
                            walletBalance?.ETH.decimals ?? 0,
                          )}{' '}
                          ETH
                        </div>
                      </div>
                    </div>

                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <div className="flex size-9 items-center justify-center rounded-full border bg-black">
                          <USDCIcon className="size-5" />
                        </div>
                        <div>
                          <div className="font-medium">USD Coin</div>
                          <div className="text-text-secondary text-sm">
                            USDC
                          </div>
                        </div>
                      </div>
                      <div className="text-right">
                        <div className="font-medium">
                          {formatBalanceAmount(
                            walletBalance?.USDC.amount ?? '0',
                            walletBalance?.USDC.decimals ?? 0,
                          )}{' '}
                          USDC
                        </div>
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card>

              {/* <Card>
                <CardHeader>
                  <CardTitle className="text-lg">Recent Activity</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-4">
                    <div className="flex items-center gap-3">
                      <div className="flex h-8 w-8 items-center justify-center rounded-full bg-green-100">
                        <ArrowDownLeft className="h-4 w-4 text-green-600" />
                      </div>
                      <div className="flex-1">
                        <div className="text-sm font-medium">Received USDC</div>
                        <div className="text-text-secondary text-xs">
                          2 hours ago
                        </div>
                      </div>
                      <div className="text-sm font-medium text-green-600">
                        +9.98 USDC
                      </div>
                    </div>

                    <div className="flex items-center gap-3">
                      <div className="flex h-8 w-8 items-center justify-center rounded-full bg-green-100">
                        <ArrowDownLeft className="h-4 w-4 text-green-600" />
                      </div>
                      <div className="flex-1">
                        <div className="text-sm font-medium">Received ETH</div>
                        <div className="text-text-secondary text-xs">
                          1 day ago
                        </div>
                      </div>
                      <div className="text-sm font-medium text-green-600">
                        +0.003 ETH
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card> */}
            </div>
          </div>
        </>
      ) : (
        <div className="flex h-full flex-col items-center justify-center">
          <div className="col-span-4 flex flex-col items-center justify-center gap-3 rounded-md p-6">
            <AddCryptoWalletIcon />
            <div className="flex flex-col items-center text-center">
              <h2 className="text-lg font-medium">
                {t('settings.cryptoWallet.emptyState.title')}
              </h2>
              <p className="text-text-secondary text-sm">
                {t('settings.cryptoWallet.emptyState.description')}
              </p>
            </div>
            <div className="flex items-center gap-3">
              <CreateWalletDialog buttonLabel="Setup Wallet" />
            </div>
          </div>
        </div>
      )}

      {walletInfo?.payment_wallet?.data?.mnemonic && (
        <Dialog
          open={showRecoveryPhraseModal}
          onOpenChange={setShowRecoveryPhraseModal}
        >
          <DialogContent showCloseButton className="w-full p-6">
            <SecretRecoveryPhraseDisplay
              mnemonic={walletInfo?.payment_wallet?.data?.mnemonic ?? ''}
            />
            <DialogClose asChild>
              <Button
                className="w-full"
                size="md"
                onClick={() => setShowRecoveryPhraseModal(false)}
              >
                Done
              </Button>
            </DialogClose>
          </DialogContent>
        </Dialog>
      )}
    </SimpleLayout>
  );
};

const CreateWalletDialog = ({ buttonLabel }: { buttonLabel: string }) => {
  const [elementRef, bounds] = useMeasure();
  const previousHeightRef = useRef<number | null>(null);

  const openWalletCreationModal = useWalletsStore(
    (state) => state.openWalletCreationModal,
  );
  const setOpenWalletCreationModal = useWalletsStore(
    (state) => state.setOpenWalletCreationModal,
  );
  const resetWalletCreation = useWalletsStore(
    (state) => state.resetWalletCreation,
  );
  const walletCreationView = useWalletsStore(
    (state) => state.walletCreationView,
  );
  const setWalletCreationView = useWalletsStore(
    (state) => state.setWalletCreationView,
  );

  const handleBack = () => {
    if (
      walletCreationView === WalletCreateConnectView.RegularMnemonic ||
      walletCreationView === WalletCreateConnectView.RegularPrivateKey ||
      walletCreationView === WalletCreateConnectView.RegularCreate ||
      walletCreationView === WalletCreateConnectView.ViewSecretRecoveryPhrase
    ) {
      setWalletCreationView(WalletCreateConnectView.Main);
      return;
    }
    setWalletCreationView(WalletCreateConnectView.Main);
  };

  const opacityDuration = useMemo(() => {
    const currentHeight = bounds.height ?? 0;
    const previousHeight = previousHeightRef.current ?? 0;

    const MIN_DURATION = 0.15;
    const MAX_DURATION = 0.27;

    if (!previousHeightRef.current) {
      previousHeightRef.current = currentHeight;
      return MIN_DURATION;
    }

    const heightDifference = Math.abs(currentHeight - previousHeight);
    previousHeightRef.current = currentHeight;

    const duration = Math.min(
      Math.max(heightDifference / 500, MIN_DURATION),
      MAX_DURATION,
    );

    return duration;
  }, [bounds.height]);

  const renderContent = () => {
    switch (walletCreationView) {
      case WalletCreateConnectView.Main:
        return (
          <div>
            <DialogHeader>
              <DialogTitle className="text-center">
                Create / Connect Wallet
              </DialogTitle>
            </DialogHeader>
            <div className="mt-8 space-y-3">
              <Button
                className="flex h-[auto] w-full items-center justify-start gap-4 rounded-md px-5 py-2.5 text-left"
                onClick={() =>
                  setWalletCreationView(WalletCreateConnectView.RegularCreate)
                }
                variant="outline"
              >
                <PlusIcon className="size-4 shrink-0" />
                <div>
                  <div className="text-sm font-semibold">Create New</div>
                  <div className="text-text-secondary text-sm">
                    Create a new wallet to store your assets.
                  </div>
                </div>
              </Button>
              <Button
                className="flex h-[auto] w-full items-center justify-start gap-4 rounded-md px-5 py-2.5 text-left"
                onClick={() =>
                  setWalletCreationView(WalletCreateConnectView.RegularMnemonic)
                }
                variant="outline"
              >
                <FileText className="size-4 shrink-0" />
                <div>
                  <div className="text-sm font-semibold">
                    Import Secret Recovery Phrase
                  </div>
                  <div className="text-text-secondary text-sm">
                    Restore to regain access to your cryptocurrency assets.
                  </div>
                </div>
              </Button>
              <Button
                className="flex h-[auto] w-full items-center justify-start gap-4 rounded-md px-5 py-2.5 text-left"
                onClick={() =>
                  setWalletCreationView(
                    WalletCreateConnectView.RegularPrivateKey,
                  )
                }
                variant="outline"
              >
                <Download className="size-4 shrink-0" />
                <div>
                  <div className="text-sm font-semibold">
                    {' '}
                    Import Private Key
                  </div>
                  <div className="text-text-secondary text-sm">
                    Restore to regain access to your cryptocurrency assets.
                  </div>
                </div>
              </Button>
            </div>
          </div>
        );
      case WalletCreateConnectView.RegularCreate:
        return <RegularCreateWallet />;
      case WalletCreateConnectView.RegularMnemonic:
        return <RegularRestoreWalletMnemonic />;
      case WalletCreateConnectView.RegularPrivateKey:
        return <RegularRestoreWalletPrivateKey />;
      case WalletCreateConnectView.ViewSecretRecoveryPhrase:
        return <RegularRestoreWalletSecretRecoveryPhrase />;
      default:
        throw new Error('Invalid view');
    }
  };

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open) {
          resetWalletCreation();
          return;
        }
        setOpenWalletCreationModal(open);
      }}
      open={openWalletCreationModal}
    >
      <DialogTrigger asChild>
        <Button size="sm">{buttonLabel}</Button>
      </DialogTrigger>
      <DialogContent
        onInteractOutside={(e) => {
          e.preventDefault();
        }}
      >
        <motion.div
          animate={{
            height: bounds.height ?? 0,
            transition: {
              duration: 0.27,
              ease: [0.25, 1, 0.5, 1],
            },
          }}
        >
          {walletCreationView !== WalletCreateConnectView.Main && (
            <Button
              className="absolute top-6 left-4"
              onClick={handleBack}
              size="icon"
              variant="tertiary"
            >
              <ArrowLeft className="h-4 w-4" />
            </Button>
          )}
          <DialogClose asChild>
            <Button
              className="absolute top-6 right-4"
              size="icon"
              variant="tertiary"
            >
              <XIcon className="text-text-secondary h-5 w-5" />
            </Button>
          </DialogClose>
          <div className="px-2 pt-2.5 antialiased" ref={elementRef}>
            <AnimatePresence
              custom={walletCreationView}
              initial={false}
              mode="popLayout"
            >
              <motion.div
                animate={{ opacity: 1, scale: 1, y: 0 }}
                exit={{ opacity: 0, scale: 0.96 }}
                initial={{ opacity: 0, scale: 0.96 }}
                key={walletCreationView}
                transition={{
                  duration: opacityDuration,
                  ease: [0.26, 0.08, 0.25, 1],
                }}
              >
                {renderContent()}
              </motion.div>
            </AnimatePresence>
          </div>
        </motion.div>
      </DialogContent>
    </Dialog>
  );
};

export const mpcRestoreWalletFormSchema = z.object({
  name: z.string().min(1),
  privateKey: z.string().min(1),
  walletId: z.string(),
  serverSigner: z.boolean(),
});

export type MpcRestoreWalletFormSchema = z.infer<
  typeof mpcRestoreWalletFormSchema
>;

const MpcRestoreWallet = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const form = useForm<MpcRestoreWalletFormSchema>({
    resolver: zodResolver(mpcRestoreWalletFormSchema),
    defaultValues: {
      serverSigner: false,
    },
  });
  const resetWalletCreation = useWalletsStore(
    (state) => state.resetWalletCreation,
  );

  const { mutateAsync: restoreCoinbaseMPCWallet } = useRestoreCoinbaseMpcWallet(
    {
      onSuccess: () => {
        resetWalletCreation();
      },
      onError: (error) => {
        toast.error('Error restoring MPC wallet', {
          description: error?.response?.data?.message ?? error.message,
        });
      },
    },
  );

  const handleSubmit = async (data: MpcRestoreWalletFormSchema) => {
    await restoreCoinbaseMPCWallet({
      token: auth?.api_v2_key ?? '',
      nodeAddress: auth?.node_address ?? '',
      network: NetworkIdentifier.BaseSepolia, // You might want to make this configurable
      name: data.name,
      privateKey: data.privateKey,
      walletId: data.walletId,
      useServerSigner: data.serverSigner.toString(),
      role: WalletRole.Both, // You might want to make this configurable
    });
  };

  return (
    <div>
      <DialogHeader>
        <DialogTitle className="text-center">Restore MPC Wallet</DialogTitle>
      </DialogHeader>
      <Form {...form}>
        <form
          className="flex flex-col gap-6 pt-8"
          onSubmit={form.handleSubmit(handleSubmit)}
        >
          <FormField
            control={form.control}
            name="name"
            render={({ field }) => <TextField field={field} label="Name" />}
          />
          <FormField
            control={form.control}
            name="privateKey"
            render={({ field }) => (
              <TextField field={field} label="Private Key" type="password" />
            )}
          />
          <FormField
            control={form.control}
            name="walletId"
            render={({ field }) => (
              <TextField field={field} label="Wallet ID" />
            )}
          />

          <FormField
            control={form.control}
            name="serverSigner"
            render={({ field }) => (
              <FormItem className="mt-4 flex flex-row items-center justify-start space-x-3 py-1">
                <FormControl>
                  <Switch
                    checked={field.value}
                    id={'custom-model'}
                    onCheckedChange={field.onChange}
                  />
                </FormControl>
                <div
                  className={cn(
                    'text-text-secondary space-y-1 text-sm leading-none',
                    field.value && 'text-white',
                  )}
                >
                  <label htmlFor="custom-model">Enable Server Signer</label>
                </div>
              </FormItem>
            )}
          />
          <div className="flex justify-end pt-6">
            <div className="flex justify-end gap-2">
              <Button className="min-w-[100px] flex-1" size="sm" type="submit">
                {t('common.restore')}
              </Button>
            </div>
          </div>
        </form>
      </Form>
    </div>
  );
};

export const regularRestoreWalletMnemonicFormSchema = z.object({
  role: z.string().min(1),
  network: z.string().min(1),
  mnemonic: z.string().min(1),
});

export type RegularRestoreWalletMnemonicFormSchema = z.infer<
  typeof regularRestoreWalletMnemonicFormSchema
>;

const RegularRestoreWalletMnemonic = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const resetWalletCreation = useWalletsStore(
    (state) => state.resetWalletCreation,
  );
  const form = useForm<RegularRestoreWalletMnemonicFormSchema>({
    resolver: zodResolver(regularRestoreWalletMnemonicFormSchema),
    defaultValues: {
      network: NetworkIdentifier.BaseSepolia,
      role: WalletRole.Both,
    },
  });
  const [showMnemonic, setShowMnemonic] = useState(false);

  const { mutateAsync: restoreLocalWallet, isPending } = useRestoreLocalWallet({
    onSuccess: () => {
      resetWalletCreation();
    },
    onError: (error) => {
      toast.error('Error restoring wallet', {
        description: error?.response?.data?.message ?? error.message,
      });
    },
  });

  const handleSubmit = async (data: RegularRestoreWalletMnemonicFormSchema) => {
    await restoreLocalWallet({
      token: auth?.api_v2_key ?? '',
      nodeAddress: auth?.node_address ?? '',
      network: data.network as NetworkIdentifier,
      role: data.role as WalletRole,
      mnemonic: data.mnemonic,
    });
  };
  return (
    <div>
      <DialogHeader>
        <DialogTitle className="text-center">Import Secret Phrase</DialogTitle>
      </DialogHeader>
      <Form {...form}>
        <form
          className="flex flex-col gap-6 pt-8"
          onSubmit={form.handleSubmit(handleSubmit)}
        >
          <FormField
            control={form.control}
            name="mnemonic"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Secret Recovery Phrase</FormLabel>
                <div className="relative">
                  <FormControl>
                    <Textarea
                      className="!min-h-[130px] resize-none text-sm"
                      spellCheck={false}
                      style={
                        {
                          WebkitTextSecurity: showMnemonic ? 'none' : 'disc',
                        } as React.CSSProperties
                      }
                      {...field}
                    />
                  </FormControl>
                  <Button
                    aria-label={showMnemonic ? 'Hide phrase' : 'Show phrase'}
                    className="absolute top-2 right-2"
                    onClick={() => setShowMnemonic(!showMnemonic)}
                    size="icon"
                    type="button"
                    variant="ghost"
                  >
                    {showMnemonic ? (
                      <EyeOffIcon aria-hidden="true" className="h-4 w-4" />
                    ) : (
                      <EyeIcon aria-hidden="true" className="h-4 w-4" />
                    )}
                  </Button>
                </div>
              </FormItem>
            )}
          />

          <div className="flex justify-end pt-6">
            <div className="flex justify-end gap-2">
              <Button
                className="min-w-[100px] flex-1"
                size="sm"
                type="submit"
                isLoading={isPending}
                disabled={isPending}
              >
                {t('common.restore')}
              </Button>
            </div>
          </div>
        </form>
      </Form>
    </div>
  );
};

export const regularRestoreWalletPrivateKeyFormSchema = z.object({
  role: z.string().min(1),
  network: z.string().min(1),
  privateKey: z.string().min(1),
});

export type RegularRestoreWalletPrivateKeyFormSchema = z.infer<
  typeof regularRestoreWalletPrivateKeyFormSchema
>;

const RegularRestoreWalletPrivateKey = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const resetWalletCreation = useWalletsStore(
    (state) => state.resetWalletCreation,
  );

  const form = useForm<RegularRestoreWalletPrivateKeyFormSchema>({
    resolver: zodResolver(regularRestoreWalletPrivateKeyFormSchema),
    defaultValues: {
      network: NetworkIdentifier.BaseSepolia,
      role: WalletRole.Both,
    },
  });

  const { mutateAsync: restoreLocalWallet } = useRestoreLocalWallet({
    onSuccess: () => {
      resetWalletCreation();
    },
    onError: (error) => {
      toast.error('Error restoring wallet', {
        description: error?.response?.data?.message ?? error.message,
      });
    },
  });

  const handleSubmit = async (
    data: RegularRestoreWalletPrivateKeyFormSchema,
  ) => {
    await restoreLocalWallet({
      token: auth?.api_v2_key ?? '',
      nodeAddress: auth?.node_address ?? '',
      network: data.network as NetworkIdentifier,
      role: data.role as WalletRole,
      privateKey: data.privateKey,
    });
  };
  return (
    <div>
      <DialogHeader>
        <DialogTitle className="text-center">Import Private Key</DialogTitle>
      </DialogHeader>
      <Form {...form}>
        <form
          className="flex flex-col gap-6 pt-8"
          onSubmit={form.handleSubmit(handleSubmit)}
        >
          <FormField
            control={form.control}
            name="network"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Select a network</FormLabel>
                <Select onValueChange={field.onChange} value={field.value}>
                  <FormControl>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                  </FormControl>

                  <SelectContent>
                    <SelectItem value="base-sepolia">Base Sepolia</SelectItem>
                    <SelectItem disabled value="disabled">
                      Other networks coming up soon
                    </SelectItem>
                  </SelectContent>
                </Select>
              </FormItem>
            )}
          />
          {/*<FormField*/}
          {/*  control={form.control}*/}
          {/*  name="role"*/}
          {/*  render={({ field }) => (*/}
          {/*    <div className="space-y-1.5">*/}
          {/*      <Label className="text-text-secondary pb-2 pl-2 text-xs">*/}
          {/*        Select account type*/}
          {/*      </Label>*/}
          {/*      <FormItem>*/}
          {/*        <FormControl>*/}
          {/*          <RadioGroup*/}
          {/*            className="grid grid-cols-3 gap-4"*/}
          {/*            id="account-type"*/}
          {/*            onValueChange={(value) => field.onChange(value)}*/}
          {/*            value={field.value || ''}*/}
          {/*          >*/}
          {/*            {[*/}
          {/*              {*/}
          {/*                label: 'Payment',*/}
          {/*                value: WalletRole.Payment,*/}
          {/*                Icon: ArrowUpFromLine,*/}
          {/*              },*/}
          {/*              {*/}
          {/*                label: 'Receive',*/}
          {/*                value: WalletRole.Receiving,*/}
          {/*                Icon: ArrowDownToLine,*/}
          {/*              },*/}
          {/*              {*/}
          {/*                label: 'Both',*/}
          {/*                value: WalletRole.Both,*/}
          {/*                Icon: ArrowLeftRight,*/}
          {/*              },*/}
          {/*            ].map(({ label, value, Icon }) => (*/}
          {/*              <div key={label}>*/}
          {/*                <RadioGroupItem*/}
          {/*                  className="peer sr-only"*/}
          {/*                  id={value}*/}
          {/*                  value={value}*/}
          {/*                />*/}
          {/*                <Label*/}
          {/*                  className="peer-data-[state=checked]:border-brand [&:has([data-state=checked])]:border-brand hover:bg-gray-450 flex cursor-pointer flex-col items-center justify-between rounded-md border-2 border-gray-200 bg-gray-400 p-4 transition-colors hover:text-white"*/}
          {/*                  htmlFor={value}*/}
          {/*                >*/}
          {/*                  <Icon className="mb-3 h-6 w-6" />*/}
          {/*                  {label}*/}
          {/*                </Label>*/}
          {/*              </div>*/}
          {/*            ))}*/}
          {/*          </RadioGroup>*/}
          {/*        </FormControl>*/}
          {/*      </FormItem>*/}
          {/*    </div>*/}
          {/*  )}*/}
          {/*/>*/}
          <FormField
            control={form.control}
            name="privateKey"
            render={({ field }) => (
              <TextField field={field} label="Private Key" type="password" />
            )}
          />

          <div className="flex justify-end pt-6">
            <div className="flex justify-end gap-2">
              <Button className="min-w-[100px] flex-1" size="sm" type="submit">
                {t('common.restore')}
              </Button>
            </div>
          </div>
        </form>
      </Form>
    </div>
  );
};

export const regularCreateWalletFormSchema = z.object({
  role: z.string().min(1),
  network: z.string().min(1),
});

export type RegularCreateWalletFormSchema = z.infer<
  typeof regularCreateWalletFormSchema
>;

const RegularCreateWallet = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const setWalletCreationView = useWalletsStore(
    (state) => state.setWalletCreationView,
  );

  const form = useForm<RegularCreateWalletFormSchema>({
    resolver: zodResolver(regularCreateWalletFormSchema),
    defaultValues: {
      network: NetworkIdentifier.BaseSepolia,
      role: WalletRole.Both,
    },
  });

  const { mutateAsync: createLocalWallet, isPending } = useCreateLocalWallet({
    onSuccess: () => {
      setWalletCreationView(WalletCreateConnectView.ViewSecretRecoveryPhrase);
    },
    onError: (error) => {
      toast.error('Error creating wallet', {
        description: error?.response?.data?.message ?? error.message,
      });
    },
  });

  const handleSubmit = async (data: RegularCreateWalletFormSchema) => {
    await createLocalWallet({
      token: auth?.api_v2_key ?? '',
      nodeAddress: auth?.node_address ?? '',
      network: data.network as NetworkIdentifier,
      role: data.role as WalletRole,
    });
  };
  return (
    <div>
      <DialogHeader>
        <DialogTitle className="text-center">Create Wallet</DialogTitle>
      </DialogHeader>
      <Form {...form}>
        <form
          className="flex flex-col gap-6 pt-8"
          onSubmit={form.handleSubmit(handleSubmit)}
        >
          <FormField
            control={form.control}
            name="network"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Select a network</FormLabel>
                <Select onValueChange={field.onChange} value={field.value}>
                  <FormControl>
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                  </FormControl>

                  <SelectContent>
                    <SelectItem value="base-sepolia">Base Sepolia</SelectItem>
                    <SelectItem disabled value="disabled">
                      Other networks coming up soon
                    </SelectItem>
                  </SelectContent>
                </Select>
              </FormItem>
            )}
          />
          <div className="flex justify-end pt-6">
            <div className="flex justify-end gap-2">
              <Button
                className="min-w-[100px] flex-1"
                isLoading={isPending}
                size="sm"
                type="submit"
                disabled={isPending}
              >
                {t('common.create')}
              </Button>
            </div>
          </div>
        </form>
      </Form>
    </div>
  );
};

const RegularRestoreWalletSecretRecoveryPhrase = () => {
  const auth = useAuth((state) => state.auth);
  const resetWalletCreation = useWalletsStore(
    (state) => state.resetWalletCreation,
  );

  const { data: walletInfo } = useGetWalletList({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const mnemonic = walletInfo?.payment_wallet?.data?.mnemonic ?? '';

  return (
    <div>
      <SecretRecoveryPhraseDisplay mnemonic={mnemonic} />
      <DialogFooter className="mt-4 gap-2">
        <Button
          size="sm"
          className="w-full"
          onClick={() => {
            resetWalletCreation();
          }}
        >
          Continue
        </Button>
      </DialogFooter>
    </div>
  );
};

export default CryptoWalletPage;
