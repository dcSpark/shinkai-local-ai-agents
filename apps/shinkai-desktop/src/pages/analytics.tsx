import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { Button } from '@shinkai_network/shinkai-ui';
import { CheckIcon, XIcon } from 'lucide-react';
import { useNavigate } from 'react-router';

import {
  COMPLETION_DESTINATION,
  OnboardingStep,
} from '../components/onboarding/constants';
import { analyticsBulletPoints } from '../constants/analytics';
import { useSettings } from '../store/settings';

const AnalyticsPage = () => {
  const navigate = useNavigate();
  const { t, Trans } = useTranslation();
  const completeStep = useSettings((state) => state.completeStep);
  const getNextStep = useSettings((state) => state.getNextStep);

  const handleOptIn = () => {
    completeStep(OnboardingStep.ANALYTICS, true);
    const nextStep = getNextStep();
    if (nextStep) {
      void navigate(nextStep.path);
    } else {
      void navigate(COMPLETION_DESTINATION);
    }
  };

  const handleOptOut = () => {
    completeStep(OnboardingStep.ANALYTICS, false);
    const nextStep = getNextStep();
    if (nextStep) {
      void navigate(nextStep.path);
    } else {
      void navigate(COMPLETION_DESTINATION);
    }
  };

  return (
    <div className="mx-auto flex h-full max-w-lg flex-col gap-8">
      <h1 className="font-clash text-4xl font-semibold">
        {t('analytics.title')}
      </h1>
      <div className="text-text-secondary flex flex-1 flex-col gap-10 text-base">
        <ul className="space-y-3">
          {analyticsBulletPoints().map((item) => (
            <li className="flex items-center gap-2" key={item}>
              <Trans
                components={{
                  check: <CheckIcon className="h-5 w-5 shrink-0" />,
                  x: <XIcon className="h-5 w-5 shrink-0" />,
                }}
                i18nKey={item}
              />
            </li>
          ))}
        </ul>
        <p>
          <Trans
            components={{
              a: (
                <a
                  className={'text-white underline'}
                  href={'https://www.shinkai.com/privacy-policy'}
                  rel="noreferrer"
                  target={'_blank'}
                />
              ),
            }}
            i18nKey="analytics.moreInfo"
          />
        </p>
      </div>

      <div className="flex items-center gap-4">
        <Button
          className="w-full"
          onClick={handleOptOut}
          size="lg"
          variant="outline"
        >
          {t('common.noThanks')}
        </Button>
        <Button className="w-full" onClick={handleOptIn} size="lg">
          {t('common.iAgree')}
        </Button>
      </div>
    </div>
  );
};

export default AnalyticsPage;
