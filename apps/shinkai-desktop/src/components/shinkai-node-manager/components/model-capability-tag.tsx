import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { Badge } from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { ALargeSmall, Brain, Cloud, Images, Wrench } from 'lucide-react';
import { type ReactNode } from 'react';

import { OllamaModelCapability } from '../../../lib/shinkai-node-manager/ollama-models';

export const ModelCapabilityTag = ({
  className,
  capability,
  ...props
}: {
  capability: OllamaModelCapability;
} & React.HTMLAttributes<HTMLDivElement>) => {
  const { t } = useTranslation();

  const capabilityMap: {
    [key in OllamaModelCapability]: { text: string; icon: ReactNode };
  } = {
    [OllamaModelCapability.ImageToText]: {
      icon: <Images className="h-3.5 w-3.5" />,
      text: t('shinkaiNode.models.labels.visionCapability'),
    },
    [OllamaModelCapability.TextGeneration]: {
      icon: <ALargeSmall className="h-3.5 w-3.5" />,
      text: t('shinkaiNode.models.labels.textCapability'),
    },
    [OllamaModelCapability.Thinking]: {
      icon: <Brain className="h-3.5 w-3.5" />,
      text: t('shinkaiNode.models.labels.thinkingCapability'),
    },
    [OllamaModelCapability.ToolCalling]: {
      icon: <Wrench className="h-3.5 w-3.5" />,
      text: t('shinkaiNode.models.labels.toolCallingCapability'),
    },
    [OllamaModelCapability.Cloud]: {
      icon: <Cloud className="h-3.5 w-3.5" />,
      text: t('shinkaiNode.models.labels.cloudCapability'),
    },
  };
  return (
    <Badge className={cn(className)} variant="tags" {...props}>
      {capabilityMap[capability].icon}
      <span className="ml-2">{capabilityMap[capability].text}</span>
    </Badge>
  );
};
