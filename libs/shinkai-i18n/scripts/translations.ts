/* eslint-disable @typescript-eslint/no-var-requires */
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { consola } from 'consola';
import { colors } from 'consola/utils';
import { diff } from 'just-diff';
import unset from 'lodash/unset';

import i18nConfig from '../.i18nrc';
import { divider, readJSON, tagWhite, writeJSON } from '../src/lib/utils';

const DEFAULT_LOCALE = 'en-US';
const defaultLocales = resolve(__dirname, '../src/lib/default');

export const entryLocaleJsonFilepath = () =>
  resolve(__dirname, '../locales', 'en-US.json');

export const outputLocaleJsonFilepath = (locale: string) =>
  resolve(__dirname, '../locales', `${locale}.json`);

const genDiff = () => {
  /* Compare dev and prod version to remove if any */

  divider('Initiating locale diff');
  const resources = require(defaultLocales);
  const devJSON = resources.default;
  const filepath = entryLocaleJsonFilepath();
  if (!existsSync(filepath)) return;

  const previousProdJSON = readJSON(filepath);
  const diffResult = diff(previousProdJSON, devJSON as any);
  if (diffResult.length === 0) {
    consola.success(tagWhite(DEFAULT_LOCALE), colors.gray(filepath));
    return;
  }

  const clearLocals: string[] = [];

  for (const locale of i18nConfig.outputLocales) {
    const localeFilepath = outputLocaleJsonFilepath(locale);
    if (!existsSync(localeFilepath)) continue;
    const localeJSON = readJSON(localeFilepath);

    for (const item of diffResult) {
      unset(localeJSON, item.path);
    }

    writeJSON(localeFilepath, localeJSON);
    clearLocals.push(locale);
  }
  consola.info('clear', clearLocals);
  consola.success(tagWhite(DEFAULT_LOCALE), colors.gray(filepath), {
    remove: diffResult.length,
  });
};

/* Generate default locale */
const genTranslations = () => {
  divider(`Generating default locale: ${DEFAULT_LOCALE}`);

  const resources = require(defaultLocales);
  const dataJSON = resources.default;
  const filepath = entryLocaleJsonFilepath();
  if (!existsSync(filepath)) return;
  writeJSON(filepath, dataJSON);
  consola.success(tagWhite(DEFAULT_LOCALE), colors.gray(filepath));
};

const main = () => {
  genDiff();
  genTranslations();
};

main();
