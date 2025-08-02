import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  Card,
  RadioGroup,
  RadioGroupItem,
  Label,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';

import { type ChatFontSize, useSettings } from '../store/settings';
import { SimpleLayout } from './layout/simple-layout';

const AppearancePage = () => {
  const { t } = useTranslation();

  const chatFontSize = useSettings((state) => state.chatFontSize);
  const setChatFontSize = useSettings((state) => state.setChatFontSize);

  return (
    <SimpleLayout classname="max-w-xl" title={t('settings.appearance.label')}>
      <div className="space-y-6">
        <div>
          <h2 className="text-lg font-semibold text-white">Chat font</h2>
          <p className="text-text-secondary text-sm">
            {t('settings.appearance.chatFontSize.description')}
          </p>
        </div>

        <RadioGroup
          className="grid grid-cols-4 gap-4"
          onValueChange={(value) => setChatFontSize(value as ChatFontSize)}
          value={chatFontSize}
        >
          <Label
            className={cn(
              'cursor-pointer space-y-2',
              chatFontSize === 'sm' && '[&_div]:border-brand',
            )}
            htmlFor="sm"
          >
            <RadioGroupItem className="sr-only" id="sm" value="sm" />
            <Card className="flex aspect-square flex-col items-center justify-center gap-2 rounded-lg border-2 p-4">
              <span className="font-inter flex min-h-[40px] items-center justify-center text-sm">
                Aa
              </span>
              <span className="text-sm font-medium">
                {t('settings.appearance.chatFontSize.small')}
              </span>
            </Card>
          </Label>

          <Label
            className={cn(
              'cursor-pointer space-y-2',
              chatFontSize === 'base' && '[&_div]:border-brand',
            )}
            htmlFor="base"
          >
            <RadioGroupItem className="sr-only" id="base" value="base" />
            <Card className="flex aspect-square flex-col items-center justify-center gap-2 rounded-lg border-2 p-4">
              <span className="font-inter flex min-h-[40px] items-center justify-center text-base">
                Aa
              </span>
              <span className="text-sm font-medium">
                {t('settings.appearance.chatFontSize.medium')}
              </span>
            </Card>
          </Label>

          <Label
            className={cn(
              'cursor-pointer space-y-2',
              chatFontSize === 'lg' && '[&_div]:border-brand',
            )}
            htmlFor="lg"
          >
            <RadioGroupItem className="sr-only" id="lg" value="lg" />
            <Card className="flex aspect-square flex-col items-center justify-center gap-2 rounded-lg border-2 p-4">
              <span className="font-inter flex min-h-[40px] items-center justify-center text-lg">
                Aa
              </span>
              <span className="text-sm font-medium">
                {t('settings.appearance.chatFontSize.large')}
              </span>
            </Card>
          </Label>

          <Label
            className={cn(
              'cursor-pointer space-y-2',
              chatFontSize === 'xl' && '[&_div]:border-brand',
            )}
            htmlFor="xl"
          >
            <RadioGroupItem className="sr-only" id="xl" value="xl" />
            <Card className="flex aspect-square flex-col items-center justify-center gap-2 rounded-lg border-2 p-4">
              <span className="font-inter flex min-h-[40px] items-center justify-center text-xl">
                Aa
              </span>
              <span className="text-sm font-medium">
                {t('settings.appearance.chatFontSize.extraLarge')}
              </span>
            </Card>
          </Label>
        </RadioGroup>
      </div>
    </SimpleLayout>
  );
};

export default AppearancePage;
