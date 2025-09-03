import { CodeLanguage } from '@shinkai_network/shinkai-message-ts/api/tools/types';

export function extractCodeLanguage(input: string): CodeLanguage | null {
  const match = input.match(/```(\w+)/);
  if (!match) {
    return null;
  }
  const [, language] = match;
  if (language === 'typescript') {
    return CodeLanguage.Typescript;
  }
  if (language === 'python') {
    return CodeLanguage.Python;
  }
  return null;
}

export function extractCodeByLanguage(message: string, language: CodeLanguage) {
  const codeBlockRegex = new RegExp(
    `\`\`\`${language.toLowerCase()}\\n([\\s\\S]*?)\\n\`\`\``,
  );
  const tsCodeMatch = message.match(codeBlockRegex);

  if (tsCodeMatch) {
    return tsCodeMatch[1].trim();
  }

  return message.trim();
}

export function detectLanguage(code: string): string {
  try {
    if (!code || typeof code !== 'string') {
      return 'Unknown';
    }

    const typeScriptRegex = new RegExp(
      [
        '\\binterface\\s+\\w+\\s*{', // interface Foo {
        '\\btype\\s+\\w+\\s*=\\s*', // type Foo =
        'function\\s+\\w+\\s*<\\w+>', // function foo<T>
        '\\bimport\\s+[{\\w\\s,]*}\\s+from', // import { x } from
        '\\w+\\s*:\\s*(string|number|boolean|any|unknown|never)', // type annotations
      ].join('|'),
      'm',
    );

    const pythonRegex = new RegExp(
      [
        '^\\s*def\\s+\\w+\\s*\\(', // def func(
        '^\\s*async\\s+def\\s+\\w+\\s*\\(', // async def func(
        '^\\s*from\\s+\\w+\\s+import\\s+\\w+', // from x import y
        '^\\s*import\\s+\\w+', // import x
        'print\\s*\\(', // print(
      ].join('|'),
      'm',
    );

    if (typeScriptRegex.test(code)) {
      return CodeLanguage.Typescript;
    } else if (pythonRegex.test(code)) {
      return CodeLanguage.Python;
    } else {
      return 'Unknown';
    }
  } catch (error) {
    console.error('Error detecting language:', error);
    return 'Unknown';
  }
}

export function validateCodeSnippet(
  text: string,
  language: CodeLanguage,
): boolean {
  switch (language) {
    case CodeLanguage.Python:
      return /^\s*async\s+def\s+run\(/m.test(text);
    case CodeLanguage.Typescript:
      return (
        /(interface\s+\w+\s*{)|(type\s+\w+\s*=)|(function\s+\w+\s*<\w+>)/.test(
          text,
        ) ||
        /export\s+async\s+function\s+\w+\s*\(.*\):\s*Promise<\{.*\}>/.test(text)
      );
    default:
      return false;
  }
}

export function getLanguage(language: string): CodeLanguage {
  switch (language) {
    case 'python':
      return CodeLanguage.Python;
    case 'typeScript':
      return CodeLanguage.Typescript;
    default:
      return CodeLanguage.Typescript;
  }
}

export function parseJsonFromCodeBlock(
  text: string,
): Record<string, unknown> | null {
  const regex = /```json([\s\S]*?)```/i;
  const match = text.match(regex);

  if (match) {
    const jsonString = match[1];
    const sanitized = jsonString.replace(/,\s*([\]}])/g, '$1');
    try {
      return JSON.parse(sanitized);
    } catch (error) {
      console.error('Error parseJsonFromCodeBlock:', error);
      return null;
    }
  }

  try {
    return JSON.parse(text);
  } catch (error) {
    console.error('Error parseJsonFromCodeBlock:', error);
    return null;
  }
}
