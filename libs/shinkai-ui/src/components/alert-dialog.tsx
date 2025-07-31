import * as AlertDialogPrimitive from '@radix-ui/react-alert-dialog';
import * as React from 'react';

import { cn } from '../utils';
import { buttonVariants } from './button';

const AlertDialog = AlertDialogPrimitive.Root;

const AlertDialogTrigger = AlertDialogPrimitive.Trigger;

const AlertDialogPortal = AlertDialogPrimitive.Portal;

type AlertDialogOverlayProps = React.ComponentPropsWithoutRef<
  typeof AlertDialogPrimitive.Overlay
> & {
  ref?: React.RefObject<
    React.ComponentRef<typeof AlertDialogPrimitive.Overlay>
  >;
};

const AlertDialogOverlay = ({
  className,
  children,
  ref,
  ...props
}: AlertDialogOverlayProps) => (
  <AlertDialogPrimitive.Overlay
    className={cn(
      'data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50 backdrop-blur-sm',
      'bg-bg-dark/90',
      className,
    )}
    {...props}
    ref={ref}
  />
);
AlertDialogOverlay.displayName = AlertDialogPrimitive.Overlay.displayName;

type AlertDialogContentProps = React.ComponentPropsWithoutRef<
  typeof AlertDialogPrimitive.Content
> & {
  ref?: React.RefObject<
    React.ComponentRef<typeof AlertDialogPrimitive.Content>
  >;
};

const AlertDialogContent = ({
  className,
  ref,
  ...props
}: AlertDialogContentProps) => (
  <AlertDialogPortal>
    <AlertDialogOverlay />
    <AlertDialogPrimitive.Content
      className={cn(
        'bg-bg-default data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 border-divider fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 shadow-lg duration-200 sm:max-w-lg',
        className,
      )}
      ref={ref}
      {...props}
    />
  </AlertDialogPortal>
);
AlertDialogContent.displayName = AlertDialogPrimitive.Content.displayName;

const AlertDialogHeader = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      'flex flex-col space-y-2 text-center sm:text-left',
      className,
    )}
    {...props}
  />
);
AlertDialogHeader.displayName = 'AlertDialogHeader';

const AlertDialogFooter = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div className={cn('flex', className)} {...props} />
);
AlertDialogFooter.displayName = 'AlertDialogFooter';

type AlertDialogTitleProps = React.ComponentPropsWithoutRef<
  typeof AlertDialogPrimitive.Title
> & {
  ref?: React.RefObject<React.ComponentRef<typeof AlertDialogPrimitive.Title>>;
};

const AlertDialogTitle = ({
  className,
  ref,
  ...props
}: AlertDialogTitleProps) => (
  <AlertDialogPrimitive.Title
    className={cn('text-lg font-semibold', className)}
    ref={ref}
    {...props}
  />
);
AlertDialogTitle.displayName = AlertDialogPrimitive.Title.displayName;

type AlertDialogDescriptionProps = React.ComponentPropsWithoutRef<
  typeof AlertDialogPrimitive.Description
> & {
  ref?: React.RefObject<
    React.ComponentRef<typeof AlertDialogPrimitive.Description>
  >;
};

const AlertDialogDescription = ({
  className,
  ref,
  ...props
}: AlertDialogDescriptionProps) => (
  <AlertDialogPrimitive.Description
    className={cn('text-text-secondary text-sm', className)}
    ref={ref}
    {...props}
  />
);
AlertDialogDescription.displayName =
  AlertDialogPrimitive.Description.displayName;

type AlertDialogActionProps = React.ComponentPropsWithoutRef<
  typeof AlertDialogPrimitive.Action
> & {
  ref?: React.RefObject<React.ComponentRef<typeof AlertDialogPrimitive.Action>>;
};

const AlertDialogAction = ({
  className,
  ref,
  ...props
}: AlertDialogActionProps) => (
  <AlertDialogPrimitive.Action
    className={cn(
      buttonVariants({ variant: 'default', size: 'sm' }),
      className,
    )}
    ref={ref}
    {...props}
  />
);
AlertDialogAction.displayName = AlertDialogPrimitive.Action.displayName;

type AlertDialogCancelProps = React.ComponentPropsWithoutRef<
  typeof AlertDialogPrimitive.Cancel
> & {
  ref?: React.RefObject<React.ComponentRef<typeof AlertDialogPrimitive.Cancel>>;
};

const AlertDialogCancel = ({
  className,
  ref,
  ...props
}: AlertDialogCancelProps) => (
  <AlertDialogPrimitive.Cancel
    className={cn(
      buttonVariants({ variant: 'outline', size: 'sm' }),
      className,
    )}
    ref={ref}
    {...props}
  />
);
AlertDialogCancel.displayName = AlertDialogPrimitive.Cancel.displayName;

export {
  AlertDialog,
  AlertDialogPortal,
  AlertDialogOverlay,
  AlertDialogTrigger,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogFooter,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogAction,
  AlertDialogCancel,
};
