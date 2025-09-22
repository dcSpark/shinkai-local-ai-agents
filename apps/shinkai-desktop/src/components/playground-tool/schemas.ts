import { z } from 'zod';

const semVerRegex =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d{0,2})(?:-[\da-z-]+(?:\.[\da-z-]+)*)?(?:\+[\da-z-]+(?:\.[\da-z-]+)*)?$/;

export const ToolMetadataSchema = z.object({
  name: z
    .string({ message: 'Tool name is required' })
    .min(1, 'Tool name is required')
    .max(100, 'Tool name must be less than 100 characters'),
  homepage: z.string().optional().default(''),
  description: z
    .string({ message: 'Tool description is required' })
    .min(1, 'Tool description is required')
    .max(500, 'Tool description must be less than 500 characters'),
  author: z.string(),
  version: z
    .string()
    .optional()
    .refine((v) => !v || semVerRegex.test(v), {
      message: 'Invalid version format, expected Semantic Versioning (x.y.z)',
    })
    .catch('0.0.1'),

  keywords: z.array(z.string()).default([]),

  configurations: z
    .object(
      {
        type: z.literal('object'),
        properties: z.record(z.any()),
        required: z.array(z.string()),
      },
      { message: 'Configurations are required' },
    )
    .passthrough(),
  parameters: z
    .object(
      {
        type: z.literal('object'),
        properties: z.record(z.any()),
        required: z.array(z.string()),
      },
      { message: 'Parameters are required' },
    )
    .passthrough(),
  result: z
    .object(
      {
        type: z.literal('object'),
        properties: z.record(z.any()).nullable().default({}),
        required: z.array(z.string()).default([]),
      },
      { message: 'Result are required' },
    )
    .passthrough(),

  sqlTables: z
    .array(
      z.object({
        name: z.string(),
        definition: z.string(),
      }),
    )
    .default([]),
  sqlQueries: z
    .array(
      z.object({
        name: z.string(),
        query: z.string(),
      }),
    )
    .default([]),
  tools: z.array(z.string()).default([]),
  oauth: z
    .array(
      z.object({
        name: z.string().default(''),
        version: z.string().default('2.0'),
        authorizationUrl: z.string().default(''),
        redirectUrl: z.string().default('https://secrets.shinkai.com/redirect'),
        tokenUrl: z.string().default(''),
        clientId: z.string().default('YOUR_PROVIDER_CLIENT_ID'),
        clientSecret: z.string().default('YOUR_PROVIDER_CLIENT_SECRET'),
        scopes: z.array(z.string()).default([]),
        responseType: z.string().default('code'),
        pkceType: z.string().default(''),
        refreshToken: z.string().default(''),
      }),
    )
    .nullish()
    .default([]),
  runner: z.string().default('any'),
  operating_system: z.array(z.string()).default(['linux', 'macos', 'windows']),
  tool_set: z.string().default(''),
});

export type ToolMetadataSchemaType = z.infer<typeof ToolMetadataSchema>;

export const ToolMetadataRawSchema = ToolMetadataSchema.pick({
  homepage: true,
  configurations: true,
  parameters: true,
  result: true,
  sqlTables: true,
  sqlQueries: true,
  oauth: true,
  runner: true,
  operating_system: true,
  tool_set: true,
  keywords: true,
  version: true,
});

export type ToolMetadataRawSchemaType = z.infer<typeof ToolMetadataRawSchema>;
