import { zodResolver } from '@hookform/resolvers/zod';
import { t, useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type UpdateInboxNameFormSchema,
  updateInboxNameFormSchema,
} from '@shinkai_network/shinkai-node-state/forms/chat/inbox';
import { useUpdateInboxName } from '@shinkai_network/shinkai-node-state/v2/mutations/updateInboxName/useUpdateInboxName';
import {
  Button,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  Input,
} from '@shinkai_network/shinkai-ui';
import { Edit3 } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useForm } from 'react-hook-form';

import { useAuth } from '../../store/auth';

export const InboxNameInput = ({
  closeEditable,
  inboxId,
  inboxName,
}: {
  closeEditable: () => void;
  inboxId: string;
  inboxName: string;
}) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const updateInboxNameForm = useForm<UpdateInboxNameFormSchema>({
    resolver: zodResolver(updateInboxNameFormSchema),
  });
  const { name: inboxNameValue } = updateInboxNameForm.watch();
  const { mutateAsync: updateInboxName } = useUpdateInboxName();
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (inputRef.current) {
      inputRef.current?.focus();
    }
  }, []);

  const onSubmit = async (data: UpdateInboxNameFormSchema) => {
    if (!auth) return;

    await updateInboxName({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      inboxId,
      inboxName: data.name,
    });
    closeEditable();
  };

  return (
    <Form {...updateInboxNameForm}>
      <form
        className="relative flex w-full items-center"
        onSubmit={updateInboxNameForm.handleSubmit(onSubmit)}
      >
        <div className="w-full">
          <FormField
            control={updateInboxNameForm.control}
            name="name"
            render={({ field }) => (
              <div className="flex h-[46px] items-center rounded-lg bg-gray-600">
                <Edit3 className="absolute top-1/2 left-2 h-4 w-4 -translate-y-1/2 transform text-white" />

                <FormItem className="space-y-0 pl-7 text-xs">
                  <FormLabel className="sr-only static">
                    {t('inboxes.updateName')}
                  </FormLabel>
                  <FormControl>
                    <Input
                      className="placeholder:!text-text-placeholder h-full border-none bg-transparent py-2 pr-16 text-xs caret-white focus-visible:ring-0 focus-visible:ring-white"
                      placeholder={inboxName}
                      {...field}
                      ref={inputRef}
                    />
                  </FormControl>
                </FormItem>
              </div>
            )}
          />
        </div>

        {inboxNameValue ? (
          <Button
            className="absolute top-1/2 right-1 h-8 -translate-y-1/2 transform text-xs text-white"
            size="sm"
            type="submit"
            variant="default"
          >
            {t('common.save')}
          </Button>
        ) : (
          <Button
            className="absolute top-1/2 right-1 h-8 -translate-y-1/2 transform text-xs text-white"
            onClick={closeEditable}
            size="sm"
            variant="tertiary"
          >
            {t('common.cancel')}
          </Button>
        )}
      </form>
    </Form>
  );
};
