import { zodResolver } from '@hookform/resolvers/zod';
import { DialogClose } from '@radix-ui/react-dialog';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useSubmitFeedback } from '@shinkai_network/shinkai-node-state/v2/mutations/submitFeedback/useSubmitFeedback';
import {
  Button,
  type ButtonProps,
  DialogTrigger,
  Form,
  FormField,
  FormItem,
  FormLabel,
  TextField,
  Textarea,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@shinkai_network/shinkai-ui';
import { MessageSquare, XIcon } from 'lucide-react';
import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { z } from 'zod';

const feedbackFormSchema = z.object({
  feedback: z.string().min(1),
  contact: z.string().min(1),
});

type FeedbackFormSchema = z.infer<typeof feedbackFormSchema>;

export const FeedbackModal = ({
  buttonProps,
  buttonLabel,
}: {
  buttonProps?: ButtonProps;
  buttonLabel?: string;
}) => {
  const { t } = useTranslation();
  const form = useForm<FeedbackFormSchema>({
    resolver: zodResolver(feedbackFormSchema),
    defaultValues: {
      feedback: '',
      contact: '',
    },
  });
  const [isOpen, setIsOpen] = useState(false);

  const { mutateAsync: submitFeedback, isPending } = useSubmitFeedback({
    onSuccess: () => {
      toast.success(t('feedback.form.success'));
      form.reset();
      setIsOpen(false);
    },
    onError: () => {
      toast.error(t('feedback.form.error'));
    },
  });

  const onSubmit = async (data: FeedbackFormSchema) => {
    await submitFeedback(data);
  };

  return (
    <Dialog
      onOpenChange={(open) => {
        if (open) {
          form.reset();
        }
        setIsOpen(open);
      }}
      open={isOpen}
    >
      <DialogTrigger asChild>
        <Button rounded="lg" size="sm" variant="outline" {...buttonProps}>
          <MessageSquare className="h-4 w-4" />
          {buttonLabel || t('feedback.button')}
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-xl">
        <DialogClose asChild>
          <Button
            className="absolute top-4 right-4"
            size="icon"
            variant="tertiary"
          >
            <XIcon className="text-gray-80 h-5 w-5" />
          </Button>
        </DialogClose>
        <DialogHeader>
          <DialogTitle>{t('feedback.title')}</DialogTitle>
          <DialogDescription>{t('feedback.description')}</DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form
            className="mt-4 space-y-4"
            onSubmit={form.handleSubmit(onSubmit)}
          >
            <FormField
              control={form.control}
              name="feedback"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t('feedback.form.feedbackLabel')}</FormLabel>
                  <Textarea
                    className="!min-h-[100px] text-sm"
                    id="feedback"
                    placeholder={t('feedback.form.feedbackPlaceholder')}
                    {...field}
                  />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="contact"
              render={({ field }) => (
                <TextField
                  field={field}
                  helperMessage={t('feedback.form.contactHelp')}
                  label={t('feedback.form.contactLabel')}
                />
              )}
            />

            <div className="flex justify-end gap-2">
              <Button
                className="min-w-[100px]"
                onClick={() => setIsOpen(false)}
                size="sm"
                variant="outline"
              >
                {t('common.cancel')}
              </Button>
              <Button
                className="min-w-[100px]"
                isLoading={isPending}
                size="sm"
                type="submit"
              >
                {t('feedback.form.submit')}
              </Button>
            </div>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
};
