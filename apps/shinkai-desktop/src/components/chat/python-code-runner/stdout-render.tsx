import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useState } from 'react';

export const StdoutRender = ({ stdout }: { stdout: string[] }) => {
  const i18n = useTranslation();
  const [isExpanded, setIsExpanded] = useState(false);

  if (stdout.length === 0) {
    return null;
  }

  return (
    <div className="mt-4 rounded-md border border-gray-200 bg-gray-50">
      <button
        className="flex w-full items-center justify-between p-3 text-sm text-gray-700 transition-colors duration-200 hover:bg-gray-100 focus:outline-hidden"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <span className="flex items-center">
          <span
            className={`mr-2 transition-transform duration-200 ${isExpanded ? 'rotate-90 transform' : ''}`}
          >
            ▶
          </span>
          {i18n.t('codeRunner.stdout')} ({stdout.length} line
          {stdout.length !== 1 ? 's' : ''})
        </span>
        <span className="text-gray-500">{isExpanded ? 'Hide' : 'Show'}</span>
      </button>
      {isExpanded && (
        <div className="border-t border-gray-200">
          <pre className="text-text-secondary0 max-h-60 overflow-y-auto p-4 text-sm break-words whitespace-pre-wrap">
            {stdout.map((line, index) => (
              <div className="mb-1 last:mb-0" key={index}>
                <span className="mr-2 text-gray-500 select-none">
                  {index + 1}
                </span>
                {line}
              </div>
            ))}
          </pre>
        </div>
      )}
    </div>
  );
};
