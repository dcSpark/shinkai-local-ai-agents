import { EyeIcon, EyeOffIcon } from 'lucide-react';
import * as React from 'react';
import { useEffect, useImperativeHandle, useRef } from 'react';

import { type RefCallBack } from 'react-hook-form';
import { cn } from '../utils';
import { Badge } from './badge';
import { Button } from '.';

export interface InputProps
  extends React.InputHTMLAttributes<HTMLInputElement> {
  startAdornment?: React.ReactNode;
  endAdornment?: React.ReactNode;
  hidePasswordToggle?: boolean;
  ref?: React.RefObject<HTMLInputElement | null> | RefCallBack;
}

const Input = ({
  className,
  type,
  startAdornment,
  endAdornment,
  hidePasswordToggle,
  ref,
  ...props
}: InputProps) => {
  const inputRef = useRef<HTMLInputElement>(null);
  const startAdornmentRef = useRef<HTMLDivElement>(null);
  const endAdornmentRef = useRef<HTMLDivElement>(null);
  const [showPassword, setShowPassword] = React.useState(false);

  const togglePasswordVisibility = () => {
    setShowPassword(!showPassword);
  };

  const style: React.CSSProperties = {};
  if (startAdornment) {
    style.paddingLeft = `${
      (startAdornmentRef?.current?.offsetWidth ?? 0) + 20
    }px`;
  }
  if (endAdornment) {
    style.paddingRight = `${
      (endAdornmentRef?.current?.offsetWidth ?? 0) + 20
    }px`;
  }

  useImperativeHandle(ref, () => inputRef.current!, []);

  useEffect(() => {
    if (props.autoFocus) {
      setTimeout(() => {
        // trick to wait the modal to be opened to focus
        inputRef?.current?.focus();
      }, 0);
    }
  }, [props.autoFocus]);

  const inputType = type === 'password' && showPassword ? 'text' : type;

  return (
    <>
      <input
        className={cn(
          'h-input border-border-input bg-bg-secondary focus:border-border-input-focus disabled:bg-bg-input-disabled peer placeholder-shown:border-border-input text-text-default w-full rounded-lg border px-4 py-3 pt-8 text-sm font-medium placeholder-transparent outline outline-0 transition-all placeholder-shown:border focus:border focus:outline-0 disabled:border-0 disabled:text-gray-400',
          startAdornment && 'pl-[var(--custom-padding-left-input)]',
          endAdornment && 'pr-[var(--custom-padding-right-input)]',
          type === 'password' && 'pr-[60px]',
          className,
        )}
        placeholder=" "
        ref={inputRef}
        spellCheck={false}
        style={style}
        type={inputType}
        {...props}
      />
      {startAdornment ? (
        <Badge
          className="peer/adornment adornment absolute top-[30px] left-4"
          ref={startAdornmentRef}
          variant="inputAdornment"
        >
          {startAdornment}
        </Badge>
      ) : null}
      {endAdornment && typeof endAdornment === 'string' ? (
        <Badge
          className="peer/adornment adornment absolute top-[30px] right-4"
          ref={endAdornmentRef}
          variant="inputAdornment"
        >
          {endAdornment}
        </Badge>
      ) : null}
      {endAdornment &&
      typeof endAdornment !== 'string' &&
      React.isValidElement(endAdornment)
        ? React.cloneElement(endAdornment, {
            ref: endAdornmentRef,
          } as React.ComponentProps<typeof Badge>)
        : null}
      {type === 'password' && !hidePasswordToggle && (
        <Button
          aria-label={showPassword ? 'Hide password' : 'Show password'}
          className="text-text-secondary absolute top-3 right-3"
          onClick={togglePasswordVisibility}
          size={'icon'}
          type="button"
          variant={'ghost'}
        >
          {showPassword ? (
            <EyeOffIcon aria-hidden="true" className="h-4 w-4" />
          ) : (
            <EyeIcon aria-hidden="true" className="h-4 w-4" />
          )}
        </Button>
      )}
    </>
  );
};

Input.displayName = 'Input';

export { Input };
