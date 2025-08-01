import {
  type FormContextType,
  getSubmitButtonOptions,
  type RJSFSchema,
  type StrictRJSFSchema,
  type SubmitButtonProps,
} from '@rjsf/utils';

import { cn } from '../../../utils';
import { Button } from '../../button';

export default function SubmitButton<
  T = any,
  S extends StrictRJSFSchema = RJSFSchema,
  F extends FormContextType = any,
>(props: SubmitButtonProps<T, S, F>) {
  const {
    submitText,
    norender,
    props: submitButtonProps,
  } = getSubmitButtonOptions<T, S, F>(props.uiSchema);

  if (norender) {
    return null;
  }

  return (
    <div className="flex items-center justify-end">
      <Button
        {...submitButtonProps}
        className={cn(
          'border-gray-200 text-white',
          submitButtonProps?.className,
        )}
        rounded="lg"
        size="xs"
        type="submit"
        variant="tertiary"
      >
        {submitText}
      </Button>
    </div>
  );
}
