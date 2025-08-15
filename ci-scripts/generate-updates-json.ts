import fs from 'fs';
import { formatRFC3339 } from 'date-fns';
import z from 'zod';

const envSchema = z.object({
  JSON_PATH: z.string().min(1),
  VERSION: z.string().min(5),
  NOTES: z.string().min(1),
  PUB_DATE: z.string().optional(),
  DARWIN_AARCH64_SIGNATURE: z.string().min(1),
  DARWIN_AARCH64_URL: z.string().url(),
  LINUX_x86_64_SIGNATURE: z.string().min(1),
  LINUX_x86_64_URL: z.string().url(),
  WINDOWS_x86_64_SIGNATURE: z.string().min(1),
  WINDOWS_x86_64_URL: z.string().url(),
});

type Env = z.infer<typeof envSchema>;

const env: Env = envSchema.parse(process.env);

const updatesJson = {
  version: env.VERSION,
  notes: env.NOTES,
  pub_date: env.PUB_DATE || formatRFC3339(new Date()),
  platforms: {
    'darwin-aarch64': {
      signature: env.DARWIN_AARCH64_SIGNATURE,
      url: env.DARWIN_AARCH64_URL,
    },
    'linux-x86_64': {
      signature: env.LINUX_x86_64_SIGNATURE,
      url: env.LINUX_x86_64_URL,
    },
    'windows-x86_64': {
      signature: env.WINDOWS_x86_64_SIGNATURE,
      url: env.WINDOWS_x86_64_URL,
    },
  },
};

fs.writeFileSync(env.JSON_PATH, JSON.stringify(updatesJson, null, 2));
