import {
  ariaDescribedByIds,
  enumOptionsIsSelected,
  enumOptionsValueForIndex,
  type FormContextType,
  optionId,
  type RJSFSchema,
  type StrictRJSFSchema,
  type WidgetProps,
} from '@rjsf/utils';
import { type ChangeEvent, type FocusEvent } from 'react';

export default function RadioWidget<
  T = any,
  S extends StrictRJSFSchema = RJSFSchema,
  F extends FormContextType = any,
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
}: WidgetProps<T, S, F>) {
  const { enumOptions, enumDisabled, emptyValue } = options;

  const _onChange = ({ target: { value } }: ChangeEvent<HTMLInputElement>) =>
    onChange(enumOptionsValueForIndex<S>(value, enumOptions, emptyValue));
  const _onBlur = ({ target: { value } }: FocusEvent<HTMLInputElement>) =>
    onBlur(id, enumOptionsValueForIndex<S>(value, enumOptions, emptyValue));
  const _onFocus = ({ target: { value } }: FocusEvent<HTMLInputElement>) =>
    onFocus(id, enumOptionsValueForIndex<S>(value, enumOptions, emptyValue));

  const inline = Boolean(options && options.inline);

  return (
    <div className="mb-0">
      {Array.isArray(enumOptions) &&
        enumOptions.map((option, index) => {
          const itemDisabled =
            Array.isArray(enumDisabled) &&
            enumDisabled.indexOf(option.value) !== -1;
          const checked = enumOptionsIsSelected<S>(option.value, value);

          const radio = (
            <label
              className={`block ${
                inline ? 'mr-3 inline-flex items-center' : ''
              }`}
              key={index}
            >
              <input
                aria-describedby={ariaDescribedByIds<T>(id)}
                checked={checked}
                className="border-border-input bg-background text-primary focus:ring-primary focus:ring-2"
                disabled={disabled || itemDisabled || readonly}
                id={optionId(id, index)}
                name={id}
                onBlur={_onBlur}
                onChange={_onChange}
                onFocus={_onFocus}
                required={required}
                type="radio"
                value={String(index)}
              />
              <span className="ml-2">{option.label}</span>
            </label>
          );
          return radio;
        })}
    </div>
  );
}
