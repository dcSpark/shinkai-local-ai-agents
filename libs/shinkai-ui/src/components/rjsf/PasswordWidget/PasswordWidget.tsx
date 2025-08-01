import { type WidgetProps } from '@rjsf/utils';
import { EyeIcon, EyeOffIcon } from 'lucide-react';
import { useState } from 'react';

import { cn } from '../../../utils';
import { Button } from '../../button';
import { Input } from '../../input';

export default function PasswordWidget<
  T = any,
  F extends Record<string, any> = any,
>({
  id,
  options,
  value,
  required,
  disabled,
  readonly,
  onChange,
  onBlur,
  onFocus,
  autofocus,
  schema,
  uiSchema,
  rawErrors = [],
}: WidgetProps<T, F>) {
  const [showPassword, setShowPassword] = useState(false);

  const _onChange = ({
    target: { value },
  }: React.ChangeEvent<HTMLInputElement>) =>
    onChange(value === '' ? options.emptyValue : value);
  const _onBlur = ({ target: { value } }: React.FocusEvent<HTMLInputElement>) =>
    onBlur(id, value);
  const _onFocus = ({
    target: { value },
  }: React.FocusEvent<HTMLInputElement>) => onFocus(id, value);

  const inputType = showPassword ? 'text' : 'password';

  return (
    <div className="relative flex w-full items-center">
      <Input
        aria-describedby={rawErrors?.length > 0 ? `${id}-error` : undefined}
        autoFocus={autofocus}
        className={cn('!h-11 !pt-1 pr-10 !pb-1')}
        disabled={disabled || readonly}
        hidePasswordToggle={true}
        id={id}
        key={inputType}
        name={id}
        onBlur={_onBlur}
        onChange={_onChange}
        onFocus={_onFocus}
        placeholder={uiSchema?.['ui:placeholder']}
        required={required}
        type={inputType}
        value={value || value === 0 ? value : ''}
      />
      <Button
        aria-label={showPassword ? 'Hide password' : 'Show password'}
        className="absolute top-1/2 right-2 h-7 w-7 -translate-y-1/2"
        disabled={disabled || readonly}
        onClick={() => setShowPassword(!showPassword)}
        size="icon"
        type="button"
        variant="tertiary"
      >
        {showPassword ? (
          <EyeIcon className="h-4 w-4" />
        ) : (
          <EyeOffIcon className="h-4 w-4" />
        )}
      </Button>
    </div>
  );
}
