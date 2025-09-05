import { zodResolver } from '@hookform/resolvers/zod';
import { CodeLanguage } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { z } from 'zod';

import { useSettings } from '../../../store/settings';

export const createToolCodeFormSchema = z.object({
  message: z.string().min(1),
  llmProviderId: z.string().min(1),
  tools: z.array(z.string()),
  language: z.nativeEnum(CodeLanguage),
  thinking: z.boolean().optional(),
});

export type CreateToolCodeFormSchema = z.infer<typeof createToolCodeFormSchema>;

export const useToolForm = (
  initialValues?: Partial<CreateToolCodeFormSchema>,
) => {
  const defaultAgentId = useSettings(
    (settingsStore) => settingsStore.defaultAgentId,
  );

  const form = useForm<CreateToolCodeFormSchema>({
    resolver: zodResolver(createToolCodeFormSchema),
    defaultValues: {
      message: '',
      tools: [],
      language: CodeLanguage.Typescript,
      thinking: false,
      ...initialValues,
    },
  });

  useEffect(() => {
    if (defaultAgentId) {
      form.setValue('llmProviderId', defaultAgentId);
    }
  }, [form, defaultAgentId]);

  return form;
};
