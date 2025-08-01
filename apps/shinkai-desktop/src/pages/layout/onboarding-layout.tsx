import { cn } from '@shinkai_network/shinkai-ui/utils';
import React, { useContext } from 'react';
import { Outlet } from 'react-router';

import { UpdateBanner } from '../../components/hardware-capabilities/update-banner';
import { LogoTapContext } from '../terms-conditions';

export type OnboardingLayoutProps = React.PropsWithChildren<
  React.HTMLAttributes<HTMLDivElement>
>;

const OnboardingLayout = ({ className, ...props }: OnboardingLayoutProps) => {
  const { tapCount, setTapCount, setShowLocalNodeOption } =
    useContext(LogoTapContext);

  const handleLogoTap = () => {
    const newCount = tapCount + 1;
    setTapCount(newCount);
    if (newCount >= 5) {
      setShowLocalNodeOption(true);
      setTapCount(0);
    }
  };

  return (
    <div
      className={cn(
        'bg-bg-dark relative mx-auto grid h-full grid-cols-2 flex-col-reverse items-center px-[48px]',
        className,
      )}
      {...props}
    >
      <UpdateBanner />
      <div className="flex h-[calc(100dvh-100px)] items-center justify-center">
        <div className="mx-auto flex h-[600px] w-full max-w-lg flex-col gap-12">
          <img
            alt="shinkai logo"
            className="w-24 cursor-pointer"
            data-cy="shinkai-logo"
            onClick={handleLogoTap}
            src={'./visor.svg'}
          />

          <div className="flex-1">
            <Outlet />
          </div>
        </div>
      </div>
      <div className="grid h-full place-items-center">
        <div className="relative size-full">
          <div className="absolute left-14 z-10 flex aspect-square size-full items-center object-left bg-blend-darken">
            <img
              alt="shinkai logo"
              className="size-full max-h-[70vh] object-cover object-left"
              data-cy="onboarding-logo"
              src={'./onboarding.png'}
            />
          </div>
        </div>
      </div>
    </div>
  );
};

export default OnboardingLayout;
