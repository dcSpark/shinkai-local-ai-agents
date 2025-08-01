import * as TogglePrimitive from '@radix-ui/react-toggle';
import { cva, type VariantProps } from 'class-variance-authority';
import * as React from 'react';

import { cn } from '../utils';

const toggleVariants = cva(
  'ring-offset-background hover:bg-muted hover:text-gray-80 focus-visible:ring-ring data-[state=on]:bg-official-gray-800 inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-hidden disabled:pointer-events-none disabled:opacity-50 data-[state=on]:text-white',
  {
    variants: {
      variant: {
        default: 'bg-official-gray-900',
        outline:
          'border-divider hover:bg-official-gray-900 border bg-transparent hover:text-white',
      },
      size: {
        default: 'h-10 px-3',
        sm: 'h-9 px-2.5',
        lg: 'h-11 px-5',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  },
);

const Toggle = ({
  className,
  variant,
  size,
  ...props
}: React.ComponentProps<typeof TogglePrimitive.Root> &
  VariantProps<typeof toggleVariants>) => (
  <TogglePrimitive.Root
    className={cn(toggleVariants({ variant, size, className }))}
    {...props}
  />
);

Toggle.displayName = TogglePrimitive.Root.displayName;

export { Toggle, toggleVariants };
