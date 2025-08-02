import * as React from 'react';

import { cn } from '../utils';

type CardProps = React.HTMLAttributes<HTMLDivElement> & {
  ref?: React.RefObject<HTMLDivElement>;
};

const Card = ({ className, ref, ...props }: CardProps) => (
  <div
    className={cn(
      'bg-card text-card-foreground rounded-lg border shadow-xs',
      className,
    )}
    ref={ref}
    {...props}
  />
);
Card.displayName = 'Card';

type CardHeaderProps = React.HTMLAttributes<HTMLDivElement> & {
  ref?: React.RefObject<HTMLDivElement>;
};

const CardHeader = ({ className, ref, ...props }: CardHeaderProps) => (
  <div
    className={cn('flex flex-col space-y-1.5 p-6', className)}
    ref={ref}
    {...props}
  />
);

CardHeader.displayName = 'CardHeader';

type CardTitleProps = React.HTMLAttributes<HTMLHeadingElement> & {
  ref?: React.RefObject<HTMLHeadingElement>;
};

const CardTitle = ({ className, ref, ...props }: CardTitleProps) => (
  <h3
    className={cn(
      'text-2xl leading-none font-semibold tracking-tight',
      className,
    )}
    ref={ref}
    {...props}
  />
);
CardTitle.displayName = 'CardTitle';

type CardDescriptionProps = React.HTMLAttributes<HTMLParagraphElement> & {
  ref?: React.RefObject<HTMLParagraphElement>;
};

const CardDescription = ({
  className,
  ref,
  ...props
}: CardDescriptionProps) => (
  <p
    className={cn('text-text-secondary text-sm', className)}
    ref={ref}
    {...props}
  />
);
CardDescription.displayName = 'CardDescription';

type CardContentProps = React.HTMLAttributes<HTMLDivElement> & {
  ref?: React.RefObject<HTMLDivElement>;
};

const CardContent = ({ className, ref, ...props }: CardContentProps) => (
  <div className={cn('p-6 pt-0', className)} ref={ref} {...props} />
);
CardContent.displayName = 'CardContent';

type CardFooterProps = React.HTMLAttributes<HTMLDivElement> & {
  ref?: React.RefObject<HTMLDivElement>;
};

const CardFooter = ({ className, ref, ...props }: CardFooterProps) => (
  <div
    className={cn('flex items-center p-6 pt-0', className)}
    ref={ref}
    {...props}
  />
);
CardFooter.displayName = 'CardFooter';

export {
  Card,
  CardHeader,
  CardFooter,
  CardTitle,
  CardDescription,
  CardContent,
};
