import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { LucideArrowLeft } from 'lucide-react';
import React from 'react';
import { Link, type To } from 'react-router';

export const SubpageLayout = ({
  title,
  children,
  rightElement,
  className,
  alignLeft,
}: {
  title: React.ReactNode;
  children: React.ReactNode;
  rightElement?: React.ReactNode;
  className?: string;
  alignLeft?: boolean;
}) => {
  const { t } = useTranslation();
  return (
    <div className={cn('relative container py-10', className)}>
      <div className="mb-8 flex items-center gap-2">
        <div
          className={cn(
            'flex w-full items-center gap-3',
            alignLeft ? 'justify-start' : 'justify-between',
          )}
        >
          <Link to={-1 as To}>
            <LucideArrowLeft />
            <span className="sr-only">{t('common.back')}</span>
          </Link>
          <h1
            className={cn(
              'text-center text-xl font-semibold tracking-tight',
              alignLeft && 'text-left',
            )}
          >
            {title}
          </h1>
          {alignLeft ? null : <div className="min-w-[40px]" />}
        </div>
        {rightElement}
      </div>
      <div className="flex-1">{children}</div>
    </div>
  );
};
export const FixedHeaderLayout = ({
  title,
  children,
  className,
  rightElement,
}: {
  title: React.ReactNode;
  children: React.ReactNode;
  rightElement?: React.ReactNode;
  className?: string;
}) => {
  const { t } = useTranslation();

  return (
    <div className={cn('mx-auto h-screen max-w-lg py-10 pb-4', className)}>
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Link className="" to={-1 as To}>
            <LucideArrowLeft />
            <span className="sr-only">{t('common.back')}</span>
          </Link>
          <h1 className="font-clash text-center text-2xl font-semibold tracking-tight">
            {title}
          </h1>
        </div>
        {rightElement}
      </div>
      {children}
    </div>
  );
};
export const SimpleLayout = ({
  title,
  children,
  classname,
  headerRightElement,
}: {
  title?: React.ReactNode;
  children: React.ReactNode;
  headerRightElement?: React.ReactNode;
  classname?: string;
}) => {
  return (
    <div
      className={cn(
        'flex h-full flex-col gap-4 px-2 py-4 md:container md:py-10',
        classname,
      )}
    >
      <div className="flex items-center justify-between gap-4">
        {title ? (
          <h1 className="font-clash hidden text-3xl font-medium md:block">
            {title}
          </h1>
        ) : null}
        {headerRightElement}
      </div>
      <div className="flex-1">{children}</div>
    </div>
  );
};
