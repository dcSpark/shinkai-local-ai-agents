import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type QuickConnectFormSchema,
  quickConnectFormSchema,
} from '@shinkai_network/shinkai-node-state/forms/auth/quick-connection';
import { useInitialRegistration } from '@shinkai_network/shinkai-node-state/v2/mutations/initialRegistration/useInitialRegistration';
import { useGetEncryptionKeys } from '@shinkai_network/shinkai-node-state/v2/queries/getEncryptionKeys/useGetEncryptionKeys';
import { Button, buttonVariants, Checkbox } from '@shinkai_network/shinkai-ui';
import { submitRegistrationNoCodeError } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { createContext, useContext, useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import { Link } from 'react-router';

import { OnboardingStep } from '../components/onboarding/constants';
import { ResetStorageBeforeConnectConfirmationPrompt } from '../components/reset-storage-before-connect-confirmation-prompt';
import config from '../config';
import {
  useShinkaiNodeRemoveStorageMutation,
  useShinkaiNodeSpawnMutation,
  useShinkaiNodeKillMutation,
} from '../lib/shinkai-node-manager/shinkai-node-manager-client';
import { useAuth } from '../store/auth';
import { useSettings } from '../store/settings';
import { useShinkaiNodeManager } from '../store/shinkai-node-manager';

export const LogoTapContext = createContext<{
  tapCount: number;
  setTapCount: (count: number) => void;
  showLocalNodeOption: boolean;
  setShowLocalNodeOption: (show: boolean) => void;
}>({
  tapCount: 0,
  setTapCount: () => {
    // no-op
  },
  showLocalNodeOption: false,
  setShowLocalNodeOption: () => {
    // no-op
  },
});

export const useLogoTap = () => useContext(LogoTapContext);

export const LogoTapProvider = ({
  children,
}: {
  children: React.ReactNode;
}) => {
  const [tapCount, setTapCount] = useState(0);
  const [showLocalNodeOption, setShowLocalNodeOption] = useState(false);

  return (
    <LogoTapContext.Provider
      value={{
        tapCount,
        setTapCount,
        showLocalNodeOption,
        setShowLocalNodeOption,
      }}
    >
      {children}
    </LogoTapContext.Provider>
  );
};

const TermsAndConditionsPage = () => {
  const { t, Trans } = useTranslation();
  const [termsAndConditionsAccepted, setTermsAndConditionsAccepted] =
    useState(false);
  const { showLocalNodeOption } = useLogoTap();

  const completeStep = useSettings((state) => state.completeStep);

  const termsAndConditionsAcceptedLegacy = useSettings((state) =>
    state.getTermsAndConditionsAccepted(),
  );
  const isLocalShinkaiNodeInUse = useShinkaiNodeManager(
    (state) => state.isInUse,
  );
  useEffect(() => {
    if (
      termsAndConditionsAcceptedLegacy !== undefined &&
      isLocalShinkaiNodeInUse
    ) {
      completeStep(OnboardingStep.TERMS_CONDITIONS, true);
    }
  }, [completeStep, isLocalShinkaiNodeInUse, termsAndConditionsAcceptedLegacy]);

  const setAuth = useAuth((state) => state.setAuth);
  const [
    resetStorageBeforeConnectConfirmationPromptOpen,
    setResetStorageBeforeConnectConfirmationPrompt,
  ] = useState(false);

  const { encryptionKeys } = useGetEncryptionKeys();
  const setupDataForm = useForm<QuickConnectFormSchema>({
    resolver: zodResolver(quickConnectFormSchema),
    defaultValues: {
      node_address: 'http://127.0.0.1:9550',
    },
  });

  const {
    mutateAsync: submitRegistrationNoCode,
    isPending: submitRegistrationNodeCodeIsPending,
  } = useInitialRegistration({
    onSuccess: (response, setupPayload) => {
      if (response.status === 'success' && encryptionKeys) {
        setAuth({
          api_v2_key: response.data?.api_v2_key ?? '',
          node_address: setupPayload.nodeAddress,
          profile: 'main',
          shinkai_identity: response.data?.node_name ?? '',
          encryption_pk: response.data?.encryption_public_key ?? '',
          identity_pk: response.data?.identity_public_key ?? '',
        });
        completeStep(OnboardingStep.TERMS_CONDITIONS, true);
      } else if (response.status === 'non-pristine') {
        setResetStorageBeforeConnectConfirmationPrompt(true);
      } else {
        submitRegistrationNoCodeError();
      }
    },
  });
  const { isPending: shinkaiNodeRemoveStorageIsPending } =
    useShinkaiNodeRemoveStorageMutation();
  const {
    isPending: shinkaiNodeSpawnIsPending,
    mutateAsync: shinkaiNodeSpawn,
  } = useShinkaiNodeSpawnMutation({
    onSuccess: () => {
      void onSubmit(setupDataForm.getValues());
    },
  });
  const { isPending: shinkaiNodeKillIsPending } = useShinkaiNodeKillMutation();

  const isStartLocalButtonLoading =
    shinkaiNodeSpawnIsPending ||
    shinkaiNodeKillIsPending ||
    shinkaiNodeRemoveStorageIsPending ||
    submitRegistrationNodeCodeIsPending;

  async function onSubmit(currentValues: QuickConnectFormSchema) {
    if (!encryptionKeys) return;
    await submitRegistrationNoCode({
      nodeAddress: currentValues.node_address,
      profileEncryptionPk: encryptionKeys.profile_encryption_pk,
      profileIdentityPk: encryptionKeys.profile_identity_pk,
    });
  }

  const onCancelConfirmation = () => {
    setResetStorageBeforeConnectConfirmationPrompt(false);
  };

  const onRestoreConfirmation = () => {
    setResetStorageBeforeConnectConfirmationPrompt(false);
  };

  const onResetConfirmation = () => {
    setResetStorageBeforeConnectConfirmationPrompt(false);
    void onSubmit(setupDataForm.getValues());
  };

  return (
    <div className="flex h-full flex-col justify-between gap-10">
      <div className="space-y-5">
        <h1 className="font-clash text-4xl font-semibold">
          {t('desktop.welcome')}
        </h1>
        <p className="text-text-secondary text-base">
          <Trans
            components={{
              b: <span className={'text-text-default font-semibold'} />,
            }}
            i18nKey="desktop.welcomeDescription"
          />
        </p>
      </div>
      <div className="flex gap-3">
        <Checkbox
          checked={termsAndConditionsAccepted}
          id="terms"
          onCheckedChange={(checked) =>
            setTermsAndConditionsAccepted(checked as boolean)
          }
        />
        <label
          className="inline-block cursor-pointer text-sm leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
          htmlFor="terms"
        >
          <span className={'leading-4 tracking-wide'}>
            <Trans
              components={{
                a: (
                  <a
                    className={'text-text-default underline'}
                    href={'https://www.shinkai.com/terms-of-service'}
                    rel="noreferrer"
                    target={'_blank'}
                  />
                ),
                b: (
                  <a
                    className={'text-text-default underline'}
                    href={'https://www.shinkai.com/privacy-policy'}
                    rel="noreferrer"
                    target={'_blank'}
                  />
                ),
              }}
              i18nKey="common.termsAndConditionsText"
            />
          </span>
        </label>
      </div>

      <div className="flex flex-1 flex-col justify-end gap-4">
        <Button
          className={cn(
            buttonVariants({
              variant: 'default',
              size: 'lg',
            }),
          )}
          disabled={!termsAndConditionsAccepted || isStartLocalButtonLoading}
          isLoading={isStartLocalButtonLoading}
          onClick={() => shinkaiNodeSpawn()}
        >
          {t('common.getStarted')}
        </Button>

        {(config.isDev || showLocalNodeOption) && (
          <div className="text-text-secondary items-center space-x-2 text-center text-sm">
            <span>{t('common.alreadyHaveNode')}</span>
            <Link
              className="text-text-default font-semibold underline"
              to="/quick-connection"
            >
              {t('common.quickConnect')}
            </Link>
          </div>
        )}
      </div>
      <ResetStorageBeforeConnectConfirmationPrompt
        onCancel={() => onCancelConfirmation()}
        onReset={() => onResetConfirmation()}
        onRestore={() => onRestoreConfirmation()}
        open={resetStorageBeforeConnectConfirmationPromptOpen}
      />
    </div>
  );
};

export default TermsAndConditionsPage;
