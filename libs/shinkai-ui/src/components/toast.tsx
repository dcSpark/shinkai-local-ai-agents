import React from 'react';
import { Toaster as Sonner } from 'sonner';

type ToasterProps = React.ComponentProps<typeof Sonner>;

const Toaster = ({ ...props }: ToasterProps) => {
  return (
    <Sonner
      className="toaster group"
      closeButton
      position="top-center"
      richColors
      theme={'dark' as ToasterProps['theme']}
      toastOptions={{
        classNames: {
          toast:
            'group toast group-[.toaster]:bg-background group-[.toaster]:text-text-default group-[.toaster]:border-border group-[.toaster]:shadow-lg',
          description: 'group-[.toast]:text-text-secondary',
          actionButton: 'group-[.toast]:bg-brand group-[.toast]:text-white',
          cancelButton:
            'group-[.toast]:bg-transparent group-[.toast]:text-gray=80',
        },
      }}
      visibleToasts={5}
      {...props}
    />
  );
};

export { Toaster };
