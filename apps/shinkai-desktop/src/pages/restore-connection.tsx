import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { decryptMessageWithPassphrase } from '@shinkai_network/shinkai-message-ts/cryptography';
import {
  type RestoreConnectionFormSchema,
  restoreConnectionFormSchema,
} from '@shinkai_network/shinkai-node-state/forms/settings/restore-connection';
import {
  Button,
  buttonVariants,
  ErrorMessage,
  FileUploader,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  TextField,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { ArrowLeft } from 'lucide-react';
import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { Link, type To, useNavigate } from 'react-router';

import { useAuth } from '../store/auth';

const RestoreConnectionPage = () => {
  const { t } = useTranslation();
  const setAuth = useAuth((state) => state.setAuth);
  const navigate = useNavigate();
  const [error, setError] = useState<boolean>(false);

  const form = useForm<RestoreConnectionFormSchema>({
    resolver: zodResolver(restoreConnectionFormSchema),
    defaultValues: {
      passphrase: '',
    },
  });
  const encryptedConnectionFileValue = form.watch('encryptedConnectionFile');

  const restore = async (values: RestoreConnectionFormSchema) => {
    try {
      const decryptedValue = await decryptMessageWithPassphrase(
        values.encryptedConnectionFile[0].encryptedConnection,
        values.passphrase,
      );
      if (decryptedValue) {
        const decryptedSetupData = JSON.parse(decryptedValue);
        setAuth(decryptedSetupData);
        // TODO: Add logic to test if setup data is valid to create an authenticated connection with Shinkai Node
        void navigate('/');
      }
    } catch {
      setError(true);
    }
  };

  const onConnectionFileSelected = async (files: File[]) => {
    const file = files[0];
    const reader = new FileReader();
    reader.readAsText(file, 'UTF-8');
    reader.onload = (event) => {
      if (event?.target?.readyState !== event?.target?.DONE) {
        return;
      }
      const encryptedConnection = event?.target?.result as string;
      if (!encryptedConnection.startsWith('encrypted:')) {
        return;
      }
      const fileWithEncryptedConnection = Object.assign(file, {
        encryptedConnection,
      });
      form.setValue('encryptedConnectionFile', [fileWithEncryptedConnection], {
        shouldValidate: true,
      });
    };
  };

  return (
    <div className="flex h-full flex-col space-y-3">
      <div className="mb-4 flex items-center gap-2">
        <Link
          className={cn(
            buttonVariants({
              size: 'icon',
              variant: 'tertiary',
            }),
          )}
          to={-1 as To}
        >
          <ArrowLeft className="h-6 w-6" />
        </Link>
        <h1 className="text-left text-2xl font-semibold">
          {t('restoreConnection.label')} <span aria-hidden>🔑</span>
        </h1>
      </div>
      <p>{t('restoreConnection.description')}</p>

      <Form {...form}>
        <form
          className="flex h-full flex-col justify-between space-y-2"
          onSubmit={form.handleSubmit(restore)}
        >
          <div className="flex grow flex-col space-y-3">
            <FormField
              control={form.control}
              name="encryptedConnectionFile"
              render={({ field }) => (
                <FormItem>
                  <FormLabel className="sr-only">
                    {t('restoreConnection.form.encryptedConnectionFile')}
                  </FormLabel>
                  <FormControl>
                    <div className="flex flex-col space-y-1">
                      <div className="flex items-center justify-center">
                        <FileUploader
                          accept={['.key'].join(',')}
                          descriptionText="Eg: shinkai.key"
                          maxFiles={1}
                          onChange={(acceptedFiles) => {
                            void onConnectionFileSelected(acceptedFiles);
                            field.onChange(acceptedFiles);
                          }}
                          value={field.value}
                        />
                      </div>
                      {!!encryptedConnectionFileValue?.length && (
                        <div className="truncate rounded-lg bg-gray-400 px-2 py-2 text-sm">
                          {encryptedConnectionFileValue[0].encryptedConnection}
                        </div>
                      )}
                    </div>
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="passphrase"
              render={({ field }) => (
                <TextField
                  field={field}
                  label={t('restoreConnection.form.passphrase')}
                  type={'password'}
                />
              )}
            />
            {error && <ErrorMessage message={'Invalid connection file'} />}
          </div>

          <Button className="w-full" type="submit">
            {t('restoreConnection.label')}
          </Button>
        </form>
      </Form>
    </div>
  );
};
export default RestoreConnectionPage;
