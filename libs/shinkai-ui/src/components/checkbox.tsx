import * as CheckboxPrimitive from '@radix-ui/react-checkbox';
import { CheckIcon } from '@radix-ui/react-icons';
import * as React from 'react';

import { cn } from '../utils';

type CheckboxProps = React.ComponentPropsWithoutRef<
  typeof CheckboxPrimitive.Root
> & {
  ref?: React.RefObject<React.ComponentRef<typeof CheckboxPrimitive.Root>>;
};

const Checkbox = ({ className, ref, ...props }: CheckboxProps) => (
  <CheckboxPrimitive.Root
    className={cn(
      'focus-visible:ring-ring data-[state=checked]:bg-brand data-[state=checked]:border-brand peer h-4 w-4 shrink-0 rounded-xs border border-gray-400 shadow-sm focus-visible:ring-1 focus-visible:outline-hidden disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:text-white',
      className,
    )}
    ref={ref}
    {...props}
  >
    <CheckboxPrimitive.Indicator
      className={cn('flex items-center justify-center text-current')}
    >
      <CheckIcon className="h-3.5 w-3.5" />
    </CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>
);
Checkbox.displayName = CheckboxPrimitive.Root.displayName;

export { Checkbox };
