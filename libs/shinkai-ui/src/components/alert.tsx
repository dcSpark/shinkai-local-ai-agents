import { cva, type VariantProps } from 'class-variance-authority';
import * as React from 'react';

import { cn } from '../utils';

const alertVariants = cva(
  '[&>svg]:text-text-default relative w-full rounded-lg border p-4 [&>svg]:absolute [&>svg]:top-4 [&>svg]:left-4 [&>svg+div]:translate-y-[-3px] [&>svg~*]:pl-7',
  {
    variants: {
      variant: {
        default: 'bg-bg-tertiary text-text-default',
        info: 'text-text-secondary [&>svg]:text-text-secondary bg-gray-250 border-gray-100 border-gray-200',
        destructive:
          'border-[#4d0408] bg-[#2d0607] text-[#ff9ea1] [&>svg]:text-[#ff9ea1]',
        warning:
          'border-yellow-800 bg-yellow-900 text-yellow-400 [&>svg]:text-yellow-400',
        success:
          'border-green-800 bg-green-900 text-green-400 [&>svg]:text-green-400',
        download:
          'border-gray-800 bg-gray-900 text-cyan-400 [&>svg]:text-cyan-400',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
);

type AlertProps = React.HTMLAttributes<HTMLDivElement> &
  VariantProps<typeof alertVariants> & {
    ref?: React.RefObject<HTMLDivElement>;
  };

const Alert = ({ className, variant, ref, ...props }: AlertProps) => (
  <div
    className={cn(alertVariants({ variant }), className)}
    ref={ref}
    role="alert"
    {...props}
  />
);
Alert.displayName = 'Alert';

type AlertTitleProps = React.HTMLAttributes<HTMLHeadingElement> & {
  ref?: React.RefObject<HTMLParagraphElement>;
};

const AlertTitle = ({ className, ref, ...props }: AlertTitleProps) => (
  <h5
    className={cn('mb-1 leading-none font-medium tracking-tight', className)}
    ref={ref}
    {...props}
  />
);
AlertTitle.displayName = 'AlertTitle';

type AlertDescriptionProps = React.HTMLAttributes<HTMLParagraphElement> & {
  ref?: React.RefObject<HTMLParagraphElement>;
};

const AlertDescription = ({
  className,
  ref,
  ...props
}: AlertDescriptionProps) => (
  <div
    className={cn('text-sm [&_p]:leading-relaxed', className)}
    ref={ref}
    {...props}
  />
);
AlertDescription.displayName = 'AlertDescription';

export { Alert, AlertTitle, AlertDescription };
