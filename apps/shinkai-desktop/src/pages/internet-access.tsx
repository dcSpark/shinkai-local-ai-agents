import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useGetNgrokStatus } from '@shinkai_network/shinkai-node-state/v2/queries/getNgrokStatus/useGetNgrokStatus';
import { useSetNgrokAuthToken } from '@shinkai_network/shinkai-node-state/v2/mutations/setNgrokAuthToken/useSetNgrokAuthToken';
import { useSetNgrokEnabled } from '@shinkai_network/shinkai-node-state/v2/mutations/setNgrokEnabled/useSetNgrokEnabled';

import {
  Button,
  Card,
  CardContent,
  CopyToClipboardIcon,
  Form,
  FormField,
  TextField,
} from '@shinkai_network/shinkai-ui';

import { useForm } from 'react-hook-form';

import { useAuth } from '../store/auth';
import { SimpleLayout } from './layout/simple-layout';
import z from 'zod';
import { toast } from 'sonner';
import { CheckCircle2 } from 'lucide-react';

const internetAccessSchema = z.object({
  authtoken: z.string().optional(),
});

type InternetAccessFormSchema = z.infer<typeof internetAccessSchema>;

const InternetAccessPage = () => {
  const { t, Trans } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const { data: ngrokStatus } = useGetNgrokStatus(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    },
    {
      enabled: !!auth,
      refetchInterval: 5000,
    },
  );
  const form = useForm<InternetAccessFormSchema>({
    resolver: zodResolver(internetAccessSchema),
    values: {
      authtoken: ngrokStatus?.authtoken ?? '',
    },
  });

  const { mutate: setNgrokAuthToken, isPending: isSettingNgrokAuthToken } =
    useSetNgrokAuthToken({
      onSuccess: () => {
        toast.success(t('settings.remoteAccess.success'));
      },
      onError: (error) => {
        toast.error(t('settings.remoteAccess.errors.failedToStart'), {
          description: error.response?.data?.error ?? error.message,
        });
      },
    });
  const { mutate: setNgrokEnabled, isPending: isSettingNgrokEnabled } =
    useSetNgrokEnabled({
      onSuccess: () => {
        toast.success(t('settings.remoteAccess.stopRemoteAccessSuccess'));
      },
      onError: (error) => {
        toast.error(t('settings.remoteAccess.errors.failedToStart'), {
          description: error.response?.data?.message ?? error.message,
        });
      },
    });

  const tunnelUrl = ngrokStatus?.tunnel ?? '';
  const isRunning = ngrokStatus?.enabled;

  const handleStartTunnel = async (data: InternetAccessFormSchema) => {
    if (!auth) return;

    await setNgrokAuthToken({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      authToken: data.authtoken ?? '',
    });
  };

  const handleStopTunnel = async () => {
    if (!auth) return;

    await setNgrokEnabled({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      enabled: false,
    });
  };

  return (
    <SimpleLayout classname="max-w-xl" title={t('settings.remoteAccess.title')}>
      <div className="mb-6 flex items-center justify-between">
        <p className="text-text-secondary text-base">
          <Trans
            components={{
              a: (
                <a
                  className={'text-blue-400 hover:underline'}
                  href={'https://dashboard.ngrok.com/'}
                  rel="noreferrer"
                  target={'_blank'}
                />
              ),
              b: (
                <a
                  className={'text-blue-400 hover:underline'}
                  href={
                    'https://dashboard.ngrok.com/get-started/your-authtoken'
                  }
                  rel="noreferrer"
                  target={'_blank'}
                />
              ),
            }}
            i18nKey="settings.remoteAccess.description"
          />
        </p>
      </div>
      <div className="flex flex-col pr-2.5">
        <div className="flex flex-col space-y-4">
          <Form {...form}>
            <form
              className="flex grow flex-col justify-between space-y-6 overflow-hidden"
              onSubmit={form.handleSubmit(handleStartTunnel)}
            >
              <FormField
                disabled={ngrokStatus?.enabled}
                control={form.control}
                name="authtoken"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('settings.remoteAccess.form.authToken')}
                    type="password"
                    helperMessage={
                      ngrokStatus?.enabled
                        ? t(
                            'settings.remoteAccess.form.authTokenHelperTextWhenEnabled',
                          )
                        : t('settings.remoteAccess.form.authTokenHelperText')
                    }
                  />
                )}
              />

              <div className="flex gap-4">
                {!isRunning ? (
                  <Button
                    className="w-full"
                    disabled={isSettingNgrokAuthToken}
                    isLoading={isSettingNgrokAuthToken}
                    size="md"
                    type="submit"
                  >
                    {t('settings.remoteAccess.form.enableRemoteAccess')}
                  </Button>
                ) : null}
              </div>
            </form>
          </Form>

          {ngrokStatus?.authtoken &&
            ngrokStatus?.enabled &&
            ngrokStatus?.tunnel && (
              <div className="space-y-8">
                <Card className="p-2 backdrop-blur-xl">
                  <CardContent className="space-y-3 p-2">
                    <div className="mb-4 flex items-center justify-between">
                      <div className="flex items-center space-x-2">
                        <div>
                          <h3 className="mb-2 inline-flex items-center gap-2 text-base font-bold text-white">
                            <div className="rounded-2xl border border-green-500/30 bg-green-600/30 p-1">
                              <CheckCircle2
                                className="text-green-400"
                                size={14}
                              />
                            </div>
                            {t('settings.remoteAccess.connected')}
                          </h3>
                          <p className="text-text-secondary text-sm">
                            {t(
                              'settings.remoteAccess.publicAccessURLDescription',
                            )}
                          </p>
                        </div>
                      </div>
                    </div>

                    <div className="space-y-2">
                      <label className="text-official-gray-200 block text-sm font-semibold">
                        {t('settings.remoteAccess.publicAccessUrl')}
                      </label>
                      <div className="bg-official-gray-900/60 border-divider flex items-center space-x-2 rounded-xl border p-2 shadow-inner">
                        <code className="bg-official-gray-800/50 flex-1 rounded-lg p-3 font-mono text-sm break-all text-white">
                          {tunnelUrl}
                        </code>
                        <div className="flex space-x-2">
                          <CopyToClipboardIcon string={tunnelUrl} />
                        </div>
                      </div>
                    </div>
                    <Button
                      onClick={handleStopTunnel}
                      variant="destructive"
                      size="md"
                      className="w-full"
                      isLoading={isSettingNgrokEnabled}
                      disabled={isSettingNgrokEnabled}
                    >
                      {t('settings.remoteAccess.form.stopRemoteAccess')}
                    </Button>
                  </CardContent>
                </Card>
              </div>
            )}
        </div>
      </div>
    </SimpleLayout>
  );
};

export default InternetAccessPage;
