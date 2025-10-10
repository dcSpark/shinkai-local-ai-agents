import fs from 'fs';
import path from 'path';
import axios, { AxiosError } from 'axios';
import * as cheerio from 'cheerio';
import * as z from 'zod';

const OllamaModelTag = z.object({
  name: z.string().min(1),
  size: z.string().min(1),
  hash: z.string().min(1),
  isDefault: z.boolean(),
});
const OllamaModelSchema = z.object({
  name: z.string().min(1),
  description: z.string().min(1),
  defaultTag: z.string().min(1),
  tags: z.array(OllamaModelTag).min(1),
  supportTools: z.boolean(),
  embedding: z.boolean(),
  vision: z.boolean(),
  thinking: z.boolean(),
  cloud: z.boolean(),
});

type OllamaModelTag = z.infer<typeof OllamaModelTag>;
export type OllamaModel = z.infer<typeof OllamaModelSchema>;

// Add delay utility for rate limiting
const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

// Retry logic with exponential backoff
async function retryWithBackoff<T>(
  fn: () => Promise<T>,
  retries = 3,
  delayMs = 1000,
  context = ''
): Promise<T> {
  for (let i = 0; i < retries; i++) {
    try {
      return await fn();
    } catch (error) {
      const isLastRetry = i === retries - 1;
      if (isLastRetry) {
        throw error;
      }
      
      const waitTime = delayMs * Math.pow(2, i);
      console.warn(
        `${context} - Attempt ${i + 1} failed. Retrying in ${waitTime}ms...`,
        error instanceof Error ? error.message : String(error)
      );
      await delay(waitTime);
    }
  }
  throw new Error('Unreachable');
}

const getOllamaModelsHtml = async (): Promise<string> => {
  const response = await retryWithBackoff(
    () => axios.get('https://ollama.com/library', {
      timeout: 30000,
      headers: {
        'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
      },
    }),
    3,
    2000,
    'Fetching models list'
  );
  return response.data;
};

const getOllamaModelTagsHtml = async (model: string): Promise<string> => {
  const response = await retryWithBackoff(
    () => axios.get(`https://ollama.com/library/${model}/tags`, {
      timeout: 30000,
      headers: {
        'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
      },
    }),
    3,
    2000,
    `Fetching tags for ${model}`
  );
  return response.data;
};

const modelHtmlToObject = async (
  modelElement: cheerio.Element,
  $: cheerio.CheerioAPI,
): Promise<any> => {
  const name = $(modelElement).find('div h2 span').text().trim();
  const description = $(modelElement).find('div:nth-child(1) p').text().trim();
  const uiTags: string[] = $(modelElement)
    .find('div:nth-child(2) div:nth-child(1) span')
    .toArray()
    .map((el) => $(el).text().trim());
  const supportTools = uiTags.includes('tools');
  const embedding = uiTags.includes('embedding');
  const vision = uiTags.includes('vision');
  const thinking = uiTags.includes('thinking');
  const cloud = uiTags.includes('cloud');
  const modelTagsHtml = await getOllamaModelTagsHtml(name);
  const modelTagsApi = cheerio.load(modelTagsHtml);

  const getTags = (): OllamaModelTag[] => {
    const tags: OllamaModelTag[] = modelTagsApi('.group.px-4.py-3')
      .toArray()
      .map((el) => {
        // Extract the tag name from the link or span with group-hover:underline class
        const name = modelTagsApi(el)
          .find('.group-hover\\:underline')
          .first()
          .text()
          .trim();
        
        // Extract the hash from the font-mono span
        const hash = modelTagsApi(el)
          .find('.font-mono')
          .first()
          .text()
          .trim();
        
        // Extract size from the text that contains "GB" - look in the same container as hash
        const sizeContainer = modelTagsApi(el)
          .find('.text-neutral-500')
          .text();
        const sizeMatch = sizeContainer.match(/(\d+(?:\.\d+)?\s*[GM]B)/);
        const size = sizeMatch ? sizeMatch[1] : 'Unknown';
        
        // Check if this is the default/latest tag by looking for "latest" badge
        const hasLatestBadge = modelTagsApi(el)
          .find('span')
          .toArray()
          .some((span) => modelTagsApi(span).text().trim().toLowerCase() === 'latest');
        const isDefault = hasLatestBadge;
        
        return { name, hash, size, isDefault };
      })
      .filter((tag) => tag.name && tag.name !== 'latest'); // Filter out empty names and 'latest' only tags
    
    return tags;
  };
  const tags = getTags();

  const defaultTag = tags.find((t) => t.isDefault)?.name || tags[0]?.name;
  return {
    name,
    description,
    supportTools,
    defaultTag,
    tags: tags,
    embedding,
    vision,
    thinking,
    cloud,
  };
};
const main = async () => {
  try {
    console.log('Starting Ollama models repository generation...\n');
    
    console.log('Fetching models list from ollama.com...');
    const modelsHtml = await getOllamaModelsHtml();
    const $ = cheerio.load(modelsHtml);
    const modelsElement = $('#repo ul li a');
    const modelElements = modelsElement.toArray();
    
    console.log(`Found ${modelElements.length} models. Processing...\n`);
    
    // Process models sequentially with delay to avoid overwhelming the server
    const models: OllamaModel[] = [];
    const failedModels: { name: string; error: string }[] = [];
    
    for (let i = 0; i < modelElements.length; i++) {
      const el = modelElements[i];
      const modelName = $(el).find('div h2 span').text().trim();
      
      try {
        console.log(`[${i + 1}/${modelElements.length}] Processing ${modelName}...`);
        const model = await modelHtmlToObject(el, $);
        models.push(model);
        
        // Add delay between requests to be respectful to the server
        if (i < modelElements.length - 1) {
          await delay(100); // 100ms delay between requests
        }
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        console.error(`  ✗ Failed to process ${modelName}: ${errorMsg}`);
        failedModels.push({ name: modelName, error: errorMsg });
      }
    }
    
    console.log(`\n✓ Successfully processed ${models.length} models`);
    if (failedModels.length > 0) {
      console.warn(`✗ Failed to process ${failedModels.length} models:`);
      failedModels.forEach(({ name, error }) => {
        console.warn(`  - ${name}: ${error}`);
      });
    }
    
    // Validate models
    console.log('\nValidating models...');
    const validatedModels = models
      .filter((model) => !model.name.includes('llama3.2'))
      .filter((model) => {
        const result = OllamaModelSchema.safeParse(model);
        if (result.error) {
          console.error(
            `Validation error in model ${model.name}:\n${JSON.stringify(result.error, undefined, 2)}`
          );
          return false;
        }
        return true;
      });
    
    console.log(`✓ ${validatedModels.length} models validated successfully\n`);
    
    // Write to file
    const outputPath = path.join(
      __dirname,
      '../apps/shinkai-desktop/src/lib/shinkai-node-manager/ollama-models-repository.json',
    );
    fs.writeFileSync(outputPath, JSON.stringify(validatedModels, null, 2), 'utf8');
    console.log(`✓ Models data has been written to ${outputPath}`);
    console.log(`\nTotal models saved: ${validatedModels.length}`);
    
    if (failedModels.length > 0) {
      console.warn(`\nWarning: ${failedModels.length} models could not be processed.`);
      process.exit(1);
    }
  } catch (error) {
    console.error('\n✗ Fatal error:', error instanceof Error ? error.message : String(error));
    if (error instanceof AxiosError) {
      console.error('\nNetwork error details:');
      console.error('  - Code:', error.code);
      console.error('  - Message:', error.message);
      if (error.code === 'ENOTFOUND') {
        console.error('\n  This appears to be a DNS resolution error.');
        console.error('  Please check your internet connection and DNS settings.');
        console.error('  You may also need to check if ollama.com is accessible from your network.');
      }
    }
    process.exit(1);
  }
};

void main();
