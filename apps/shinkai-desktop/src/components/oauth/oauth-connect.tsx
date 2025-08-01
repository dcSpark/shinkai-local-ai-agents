import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogHeader,
  AlertDialogTitle,
  buttonVariants,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { CheckCircle, ExternalLink, Loader2 } from 'lucide-react';
import React from 'react';

import { useOAuthSuccess } from '../../hooks/oauth';
import { useOAuth } from '../../store/oauth';

type ProviderData = {
  name: string;
  image: string;
};

const providers: Record<string, ProviderData> = {
  'github.com': {
    name: 'GitHub',
    image:
      'https://github.githubassets.com/images/modules/logos_page/GitHub-Mark.png',
  },
  // Here we can customize more known providers
};

export const OAuthConnect = () => {
  const CLOSE_TIMEOUT_SECONDS = 10;
  const { t } = useTranslation();
  const { oauthModalVisible, url, setOauthModalVisible } = useOAuth();
  const [connectDone, setConnectDone] = React.useState(false);
  const [oauthData, setoauthData] = React.useState<
    | {
        domain: string;
        state: string;
        providerData: ProviderData;
      }
    | undefined
  >(undefined);

  useOAuthSuccess((payload) => {
    console.log('oauth success', payload);
    if (payload.state === oauthData?.state) {
      setConnectDone(true);
      setTimeout(() => {
        setOauthModalVisible({ visible: false });
      }, CLOSE_TIMEOUT_SECONDS * 1000);
    }
  });

  React.useEffect(() => {
    if (url) {
      const urlParams = new URLSearchParams(new URL(url).search);
      const state = urlParams.get('state');
      console.log('Extracted state:', state);
      if (state) {
        console.log('Setting OAuth data with state and code.');
        const domain = new URL(url).hostname;
        setoauthData({
          domain: new URL(url).hostname,
          state,
          providerData: providers[domain] ?? {
            image: '',
            name: domain,
          },
        });
      } else {
        console.warn('State is missing in the URL parameters.');
      }
    }
  }, [url]);

  const [countdown, setCountdown] = React.useState(CLOSE_TIMEOUT_SECONDS);
  React.useEffect(() => {
    if (connectDone) {
      const interval = setInterval(() => {
        setCountdown((prev) => {
          if (prev <= 1) {
            clearInterval(interval);
            return 0;
          }
          return prev - 1;
        });
      }, 1000);
      return () => clearInterval(interval);
    }
  }, [connectDone]);

  React.useEffect(() => {
    if (!oauthModalVisible) {
      setConnectDone(false);
      setOauthModalVisible({ visible: false });
      setoauthData(undefined);
      setCountdown(CLOSE_TIMEOUT_SECONDS);
    }
  }, [oauthModalVisible, setOauthModalVisible]);

  return (
    <AlertDialog open={oauthModalVisible}>
      <AlertDialogContent className="rounded-lg bg-white p-6 shadow-lg">
        <AlertDialogHeader>
          <div className="flex items-center">
            {oauthData?.providerData?.image && (
              <img
                alt={`${oauthData?.providerData?.name} logo`}
                className="mr-2 h-6 w-6"
                src={oauthData.providerData.image}
              />
            )}
            <AlertDialogTitle className="text-text-secondary0 text-lg font-semibold">
              {t('oauth.title', { provider: oauthData?.providerData?.name })}
            </AlertDialogTitle>
          </div>
          <AlertDialogDescription className="text-gray-600">
            {connectDone ? (
              <div className="mt-4 flex flex-col items-start">
                <div className="flex items-center text-green-600">
                  <CheckCircle className="mr-2 h-5 w-5 animate-bounce" />
                  <span>{t('oauth.success')}</span>
                </div>
                <span className="mt-2 text-xs text-gray-500">
                  {t('oauth.successDescription', { countdown })}
                </span>
              </div>
            ) : (
              <div className="mt-4 flex flex-col items-start">
                <p>{t('oauth.navigateToProvider')}</p>
                <a
                  className={cn(
                    buttonVariants({
                      size: 'auto',
                      variant: 'link',
                    }),
                    'mt-4 rounded-lg p-0 text-sm text-inherit underline',
                  )}
                  href={url}
                  rel="noreferrer"
                  target="_blank"
                >
                  {t('oauth.goToProvider', {
                    provider: oauthData?.providerData?.name,
                  })}
                  <ExternalLink className="ml-2 inline-block h-4 w-4" />
                </a>
              </div>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="mt-4">
          {!connectDone && (
            <div className="flex items-center">
              <div className="flex flex-row items-center space-x-1">
                <Loader2 className="h-5 w-5 shrink-0 animate-spin text-gray-500" />
              </div>{' '}
              <span className="ml-2 text-gray-500">
                {t('oauth.processing')}
              </span>
            </div>
          )}
        </div>
        <div className="mt-4">
          <div className="flex justify-end">
            <button
              className="rounded-sm bg-gray-300 px-4 py-2 hover:bg-gray-400"
              onClick={() => setOauthModalVisible({ visible: false })}
            >
              Close
            </button>
          </div>
        </div>
      </AlertDialogContent>
    </AlertDialog>
  );
};
