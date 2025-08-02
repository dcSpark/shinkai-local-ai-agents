import { Slot } from '@radix-ui/react-slot';
import { ChevronRight, MoreHorizontal } from 'lucide-react';
import * as React from 'react';

import { cn } from '../utils';

type BreadcrumbProps = React.ComponentPropsWithoutRef<'nav'> & {
  separator?: React.ReactNode;
  ref?: React.RefObject<HTMLDivElement>;
};

const Breadcrumb = ({ ref, ...props }: BreadcrumbProps) => (
  <nav aria-label="breadcrumb" ref={ref} {...props} />
);

Breadcrumb.displayName = 'Breadcrumb';

type BreadcrumbListProps = React.ComponentPropsWithoutRef<'ol'> & {
  ref?: React.RefObject<HTMLOListElement>;
};

const BreadcrumbList = ({ className, ref, ...props }: BreadcrumbListProps) => (
  <ol
    className={cn(
      'text-text-secondary flex flex-wrap items-center gap-1.5 text-sm break-words sm:gap-2.5',
      className,
    )}
    ref={ref}
    {...props}
  />
);
BreadcrumbList.displayName = 'BreadcrumbList';

type BreadcrumbItemProps = React.ComponentPropsWithoutRef<'li'> & {
  ref?: React.RefObject<HTMLLIElement>;
};

const BreadcrumbItem = ({ className, ref, ...props }: BreadcrumbItemProps) => (
  <li
    className={cn('inline-flex items-center gap-1.5', className)}
    ref={ref}
    {...props}
  />
);
BreadcrumbItem.displayName = 'BreadcrumbItem';

type BreadcrumbLinkProps = React.ComponentPropsWithoutRef<'a'> & {
  asChild?: boolean;
  ref?: React.RefObject<HTMLAnchorElement>;
};

const BreadcrumbLink = ({
  asChild,
  className,
  ref,
  ...props
}: BreadcrumbLinkProps) => {
  const Comp = asChild ? Slot : 'a';

  return (
    <Comp
      className={cn('hover:text-text-default transition-colors', className)}
      ref={ref}
      {...props}
    />
  );
};
BreadcrumbLink.displayName = 'BreadcrumbLink';

type BreadcrumbPageProps = React.ComponentPropsWithoutRef<'span'> & {
  ref?: React.RefObject<HTMLSpanElement>;
};

const BreadcrumbPage = ({ className, ref, ...props }: BreadcrumbPageProps) => (
  <span
    aria-current="page"
    aria-disabled="true"
    className={cn('text-text-default font-normal', className)}
    ref={ref}
    role="link"
    {...props}
  />
);
BreadcrumbPage.displayName = 'BreadcrumbPage';

type BreadcrumbSeparatorProps = React.ComponentProps<'li'> & {
  ref?: React.RefObject<HTMLLIElement>;
};

const BreadcrumbSeparator = ({
  children,
  className,
  ...props
}: BreadcrumbSeparatorProps) => (
  <li
    aria-hidden="true"
    className={cn('[&>svg]:h-4 [&>svg]:w-4', className)}
    role="presentation"
    {...props}
  >
    {children ?? <ChevronRight />}
  </li>
);
BreadcrumbSeparator.displayName = 'BreadcrumbSeparator';

const BreadcrumbEllipsis = ({
  className,
  ...props
}: React.ComponentProps<'span'>) => (
  <span
    aria-hidden="true"
    className={cn('flex h-9 w-9 items-center justify-center', className)}
    role="presentation"
    {...props}
  >
    <MoreHorizontal className="h-4 w-4" />
    <span className="sr-only">More</span>
  </span>
);
BreadcrumbEllipsis.displayName = 'BreadcrumbElipssis';

export {
  Breadcrumb,
  BreadcrumbList,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbPage,
  BreadcrumbSeparator,
  BreadcrumbEllipsis,
};
