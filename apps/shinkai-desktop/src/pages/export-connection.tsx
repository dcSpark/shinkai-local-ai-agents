import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { encryptMessageWithPassphrase } from '@shinkai_network/shinkai-message-ts/cryptography';
import {
  type ExportConnectionFormSchema,
  exportConnectionFormSchema,
} from '@shinkai_network/shinkai-node-state/forms/settings/export-connection';
import {
  Button,
  Form,
  FormField,
  Input,
  TextField,
} from '@shinkai_network/shinkai-ui';
import {} from '@tauri-apps/api';
import { save } from '@tauri-apps/plugin-dialog';
import { BaseDirectory } from '@tauri-apps/plugin-fs';
import * as fs from '@tauri-apps/plugin-fs';
import { Download } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';

import MobileBackNavigation from '../components/mobile-back-navigation';
import { useAuth } from '../store/auth';
import { SimpleLayout } from './layout/simple-layout';

export const ExportConnection = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const form = useForm<ExportConnectionFormSchema>({
    resolver: zodResolver(exportConnectionFormSchema),
    defaultValues: {
      passphrase: '',
      confirmPassphrase: '',
    },
  });
  const passphrase = form.watch('passphrase');
  const confirmPassphrase = form.watch('confirmPassphrase');
  const [encryptedSetupData, setEncryptedSetupData] = useState<string>('');
  useEffect(() => {
    setEncryptedSetupData('');
  }, [passphrase, confirmPassphrase, setEncryptedSetupData]);
  const exportConnection = async (
    values: ExportConnectionFormSchema,
  ): Promise<void> => {
    // TODO: Convert to a common format shared by visor, app and desktop
    const parsedSetupData = JSON.stringify(auth);
    const encryptedSetupData = await encryptMessageWithPassphrase(
      parsedSetupData,
      values.passphrase,
    );
    setEncryptedSetupData(encryptedSetupData);
  };
  const download = async (): Promise<void> => {
    const content = encryptedSetupData;
    const file = new Blob([content], { type: 'text/plain' });
    const dataUrl = await new Promise<string>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.readAsDataURL(file);
    });
    const path = await save({
      defaultPath: `${auth?.shinkai_identity}.shinkai.key`
        .replace(/@/g, '')
        .replace(/\//g, '_'),
    });
    if (path) {
      const arrayBuffer = await fetch(dataUrl).then((response) =>
        response.arrayBuffer(),
      );
      const content = new Uint8Array(arrayBuffer);
      await fs.writeFile(path, content, {
        baseDir: BaseDirectory.Download,
      });
    }
  };
  return (
    <SimpleLayout classname="max-w-xl" title={t('exportConnection.label')}>
      <div className="flex grow flex-col space-y-4">
        <Form {...form}>
          <form
            className="flex flex-col justify-between space-y-8"
            onSubmit={form.handleSubmit(exportConnection)}
          >
            <div className="flex grow flex-col space-y-2">
              <FormField
                control={form.control}
                name="passphrase"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('exportConnection.form.passphrase')}
                    type="password"
                  />
                )}
              />
              <FormField
                control={form.control}
                name="confirmPassphrase"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('exportConnection.form.passphrase')}
                    type="password"
                  />
                )}
              />
            </div>
            <Button className="w-full" size="sm" type="submit">
              {t('exportConnection.generateFile')}
            </Button>
          </form>
        </Form>

        {encryptedSetupData && (
          <div className="flex grow flex-col items-center justify-center space-y-3">
            <div className="flex flex-col space-y-1">
              <span className="font-semibold">
                {t('exportConnection.downloadText')}
              </span>
              <span>{t('exportConnection.restoreText')}</span>
            </div>
            <div className="flex w-full flex-row space-x-1">
              <div className="grow cursor-pointer" onClick={() => download()}>
                <Input
                  className="cursor-pointer truncate"
                  readOnly
                  value={encryptedSetupData}
                />
              </div>
              <Button className="" onClick={() => download()}>
                <Download className="h-4 w-4" />
              </Button>
            </div>
          </div>
        )}
      </div>
    </SimpleLayout>
  );
};
