import {
  ADDITIONAL_PROPERTY_FLAG,
  type FormContextType,
  type RJSFSchema,
  type StrictRJSFSchema,
  TranslatableString,
  type WrapIfAdditionalTemplateProps,
} from '@rjsf/utils';
import { type FocusEvent } from 'react';

export default function WrapIfAdditionalTemplate<
  T = any,
  S extends StrictRJSFSchema = RJSFSchema,
  F extends FormContextType = any,
>({
  classNames,
  style,
  children,
  disabled,
  id,
  label,
  onDropPropertyClick,
  onKeyChange,
  readonly,
  required,
  schema,
  uiSchema,
  registry,
}: WrapIfAdditionalTemplateProps<T, S, F>) {
  const { templates, translateString } = registry;
  // Button templates are not overridden in the uiSchema
  const { RemoveButton } = templates.ButtonTemplates;
  const keyLabel = translateString(TranslatableString.KeyLabel, [label]);
  const additional = ADDITIONAL_PROPERTY_FLAG in schema;

  if (!additional) {
    return (
      <div className={classNames} style={style}>
        {children}
      </div>
    );
  }

  const handleBlur = ({ target }: FocusEvent<HTMLInputElement>) =>
    onKeyChange(target.value);
  const keyId = `${id}-key`;

  return (
    <div className={`flex ${classNames}`} style={style}>
      <div className="w-1/2 flex-none p-2">
        <label
          className="text-text-secondary block text-sm font-medium"
          htmlFor={keyId}
        >
          {keyLabel}
        </label>
        <input
          className="mt-1 w-full border p-2 shadow-xs"
          defaultValue={label}
          disabled={disabled || readonly}
          id={keyId}
          name={keyId}
          onBlur={!readonly ? handleBlur : undefined}
          required={required}
          type="text"
        />
      </div>
      <div className="w-1/2 flex-none p-2">{children}</div>
      <div className="w-1/4 flex-none p-2">
        <RemoveButton
          className="w-full"
          disabled={disabled || readonly}
          iconType="block"
          onClick={onDropPropertyClick(label)}
          registry={registry}
          uiSchema={uiSchema}
        />
      </div>
    </div>
  );
}
