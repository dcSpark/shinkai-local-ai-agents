import fs from 'fs';
import path from 'path';
import axios from 'axios';
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
});

type OllamaModelTag = z.infer<typeof OllamaModelTag>;
export type OllamaModel = z.infer<typeof OllamaModelSchema>;

const getOllamaModelsHtml = async (): Promise<string> => {
  const response = await axios.get('https://ollama.com/library');
  return response.data;
};

const getOllamaModelTagsHtml = async (model: string): Promise<string> => {
  const response = await axios.get(`https://ollama.com/library/${model}/tags`);
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
  };
};
const main = async () => {
  const modelsHtml = await getOllamaModelsHtml();
  const $ = cheerio.load(modelsHtml);
  const modelsElement = $('#repo ul li a');
  const models: OllamaModel[] = await Promise.all(
    modelsElement.toArray().map(async (el) => {
      const model = await modelHtmlToObject(el, $);
      return model;
    }),
  );
  models
    .filter((model) => !model.name.includes('llama3.2'))
    .forEach((model) => {
      console.log('model', model);
      const result = OllamaModelSchema.safeParse(model);
      if (result.error) {
        throw new Error(
          `Error in model ${model.name}\n${JSON.stringify(result.error, undefined, 2)}`,
        );
      }
    });
  const outputPath = path.join(
    __dirname,
    '../apps/shinkai-desktop/src/lib/shinkai-node-manager/ollama-models-repository.json',
  );
  fs.writeFileSync(outputPath, JSON.stringify(models, null, 2), 'utf8');
  console.log(`Models data has been written to ${outputPath}`);
};

main();
