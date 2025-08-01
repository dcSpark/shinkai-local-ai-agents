import {
  type FormContextType,
  type IconButtonProps,
  type RJSFSchema,
  type StrictRJSFSchema,
  TranslatableString,
} from '@rjsf/utils';
import { ArrowDownIcon, ArrowUpIcon, CopyIcon, TrashIcon } from 'lucide-react';

export default function IconButton<
  T = any,
  S extends StrictRJSFSchema = RJSFSchema,
  F extends FormContextType = any,
>(props: IconButtonProps<T, S, F>) {
  const {
    icon,
    iconType,
    className,
    uiSchema,
    registry,
    disabled,
    ...otherProps
  } = props;
  const buttonClass = iconType === 'block' ? 'w-full' : '';
  const variantClass =
    // @ts-expect-error incomplete type from rjsf
    props.variant === 'danger'
      ? 'bg-gray-400  text-text-secondary hover:text-gray-50'
      : disabled
        ? 'bg-gray-200 text-text-secondary'
        : 'bg-gray-300 hover:bg-gray-400 text-white';

  return (
    <button
      className={`grid size-8 justify-items-center rounded-full p-2 text-base font-normal [&>svg]:size-full ${buttonClass} ${variantClass} ${className}`}
      {...otherProps}
    >
      {icon}
    </button>
  );
}

export function CopyButton<
  T = any,
  S extends StrictRJSFSchema = RJSFSchema,
  F extends FormContextType = any,
>(props: IconButtonProps<T, S, F>) {
  const {
    registry: { translateString },
  } = props;
  return (
    <IconButton
      title={translateString(TranslatableString.CopyButton)}
      {...props}
      icon={<CopyIcon />}
    />
  );
}

export function MoveDownButton<
  T = any,
  S extends StrictRJSFSchema = RJSFSchema,
  F extends FormContextType = any,
>(props: IconButtonProps<T, S, F>) {
  const {
    registry: { translateString },
  } = props;
  return (
    <IconButton
      title={translateString(TranslatableString.MoveDownButton)}
      {...props}
      icon={<ArrowDownIcon />}
    />
  );
}

export function MoveUpButton<
  T = any,
  S extends StrictRJSFSchema = RJSFSchema,
  F extends FormContextType = any,
>(props: IconButtonProps<T, S, F>) {
  const {
    registry: { translateString },
  } = props;
  return (
    <IconButton
      title={translateString(TranslatableString.MoveUpButton)}
      {...props}
      icon={<ArrowUpIcon />}
    />
  );
}

export function RemoveButton<
  T = any,
  S extends StrictRJSFSchema = RJSFSchema,
  F extends FormContextType = any,
>(props: IconButtonProps<T, S, F>) {
  const {
    registry: { translateString },
  } = props;
  return (
    <IconButton
      title={translateString(TranslatableString.RemoveButton)}
      {...props}
      icon={<TrashIcon />}
      // @ts-expect-error incomplete type from rjsf
      variant="danger"
    />
  );
}
