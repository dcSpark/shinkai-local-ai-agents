import { z } from 'zod';

export const quickConnectFormSchema = z.object({
  node_address: z.url({
    error: 'Node Address must be a valid URL',
  }),
});

export type QuickConnectFormSchema = z.infer<typeof quickConnectFormSchema>;
