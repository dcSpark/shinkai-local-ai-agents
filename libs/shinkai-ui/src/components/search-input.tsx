import { SearchIcon, XIcon } from 'lucide-react';
import * as React from 'react';

import { cn } from '../utils';
import { Button, Input } from '.';

export type SearchInputProps = React.InputHTMLAttributes<HTMLInputElement> & {
  classNames?: {
    container?: string;
    input?: string;
    button?: string;
  };
};

const SearchInput = ({
  className,
  value,
  onChange,
  classNames,
  ...props
}: SearchInputProps) => {
  return (
    <div
      className={cn(
        'shadow-border-input focus-within:shadow-border-input-focus relative flex h-10 flex-1 items-center overflow-hidden rounded-full shadow-[0_0_0_1px_currentColor] transition-shadow',
        classNames?.container,
      )}
    >
      <Input
        className={cn(
          'placeholder:!text-text-placeholder !h-full border-none bg-transparent py-2 pl-10',
          classNames?.input,
        )}
        onChange={(e) => {
          onChange?.(e);
        }}
        placeholder="Search..."
        spellCheck={false}
        value={value}
        {...props}
      />
      <SearchIcon className="text-text-tertiary absolute top-1/2 left-4 -z-[1px] h-4 w-4 -translate-y-1/2" />
      {value && (
        <Button
          className={cn('absolute right-1 h-8 w-8 p-2', classNames?.button)}
          onClick={() => {
            onChange?.({
              target: { value: '' },
            } as React.ChangeEvent<HTMLInputElement>);
          }}
          size="auto"
          type="button"
          variant="tertiary"
        >
          <XIcon />
          <span className="sr-only">Clear</span>
        </Button>
      )}
    </div>
  );
};

SearchInput.displayName = 'SearchInput';

export { SearchInput };
