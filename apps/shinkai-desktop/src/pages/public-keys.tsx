import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type PublicKeysFormSchema,
  publicKeysSchema,
} from '@shinkai_network/shinkai-node-state/forms/settings/public-keys';
import {
  CopyToClipboardIcon,
  Form,
  FormField,
  TextField,
} from '@shinkai_network/shinkai-ui';
import { useForm } from 'react-hook-form';

import MobileBackNavigation from '../components/mobile-back-navigation';
import { useAuth } from '../store/auth';
import { SimpleLayout } from './layout/simple-layout';

export const PublicKeys = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const form = useForm<PublicKeysFormSchema>({
    resolver: zodResolver(publicKeysSchema),
    defaultValues: {
      node_encryption_pk: auth?.encryption_pk,
      node_signature_pk: auth?.identity_pk,
      api_v2_key: auth?.api_v2_key,
    },
  });

  return (
    <SimpleLayout classname="max-w-xl" title={t('settings.publicKeys.label')}>
      <div className="flex grow flex-col space-y-2">
        <Form {...form}>
          <form className="flex flex-col justify-between space-y-8">
            <div className="flex grow flex-col space-y-5">
              <div>
                <h2 className="text-text-secondary mb-2 text-sm">
                  {t('settings.publicKeys.nodePublicKeys')}
                </h2>
                <div className="space-y-3">
                  <FormField
                    control={form.control}
                    name="node_encryption_pk"
                    render={({ field }) => (
                      <TextField
                        classes={{
                          input: 'font-mono',
                        }}
                        endAdornment={
                          <div className="w-8">
                            <CopyToClipboardIcon
                              className="peer/adornment adornment absolute top-4 right-1 rounded-md border px-2"
                              string={field.value}
                            />
                          </div>
                        }
                        field={{ ...field, readOnly: true }}
                        label={t('settings.publicKeys.nodeEncryption')}
                      />
                    )}
                  />
                  <FormField
                    control={form.control}
                    name="node_signature_pk"
                    render={({ field }) => (
                      <TextField
                        classes={{
                          input: 'font-mono',
                        }}
                        endAdornment={
                          <div className="w-8">
                            <CopyToClipboardIcon
                              className="peer/adornment adornment absolute top-4 right-1 rounded-md border px-2"
                              string={field.value}
                            />
                          </div>
                        }
                        field={{ ...field, readOnly: true }}
                        label={t('settings.publicKeys.nodeSignature')}
                      />
                    )}
                  />
                </div>
              </div>
            </div>
            <div>
              <h2 className="text-text-secondary mb-2 text-sm">
                API Bearer Token
              </h2>
              <div className="space-y-3">
                <FormField
                  control={form.control}
                  name="api_v2_key"
                  render={({ field }) => (
                    <TextField
                      classes={{
                        input: 'font-mono',
                      }}
                      endAdornment={
                        <div className="w-8">
                          <CopyToClipboardIcon
                            className="peer/adornment adornment absolute top-4 right-1 rounded-md border px-2"
                            string={field.value}
                          />
                        </div>
                      }
                      field={{ ...field, readOnly: true }}
                      label={'Bearer Token'}
                    />
                  )}
                />
              </div>
            </div>
          </form>
        </Form>
      </div>
    </SimpleLayout>
  );
};
