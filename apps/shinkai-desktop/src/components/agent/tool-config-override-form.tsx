import { type RJSFSchema } from '@rjsf/utils';
import validator from '@rjsf/validator-ajv8';
import {
  type ShinkaiTool,
  type ToolConfigBase,
} from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { useGetTool } from '@shinkai_network/shinkai-node-state/v2/queries/getTool/useGetTool';
import {
  Button,
  generateTemplates,
  JsonForm,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
} from '@shinkai_network/shinkai-ui';
import { Trash } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useAuth } from '../../store/auth';

export const TooConfigOverrideForm = ({
  toolRouterKey,
  value,
  onChange,
}: {
  toolRouterKey: string;
  value: any;
  onChange: (e: any) => void;
}) => {
  const auth = useAuth((state) => state.auth);
  const { data, isSuccess, isPending } = useGetTool({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    toolKey: toolRouterKey ?? '',
  });
  const tool: ShinkaiTool | undefined = data?.content?.[0] as ShinkaiTool;
  const properties = useMemo(() => {
    const properties = (tool as any)?.configurations?.properties || {};
    return Object.entries(properties).map(([key, value]: [string, any]) => {
      const name = key
        ?.split(/(?=[A-Z])|_/)
        .map(
          (word: string) =>
            word.charAt(0).toUpperCase() + word.slice(1).toLowerCase(),
        )
        .join(' ');
      return {
        key,
        name,
        description: value.description,
      };
    });
  }, [tool]);

  const internalValue = useRef<any>(value);
  const updateDynamicSchema = useCallback(() => {
    const properties = (tool as any)?.configurations?.properties || {};
    const toolConfig = ((tool as any)?.config as ToolConfigBase[]) || [];
    const toolConfigAsMap = new Map(
      toolConfig.map((config) => [
        config.BasicConfig.key_name,
        config.BasicConfig,
      ]),
    );
    const propertiesToShow = Object.fromEntries(
      Object.entries(properties).filter(
        ([jsonSchemaKey, _]: [string, unknown]) => {
          const toolConfig = toolConfigAsMap.get(jsonSchemaKey);
          const requiresToolConfigurationOrOverride =
            toolConfig?.required && !toolConfig?.key_value;
          const hasOverrideValue =
            internalValue.current[jsonSchemaKey] !== undefined;
          return hasOverrideValue || requiresToolConfigurationOrOverride;
        },
      ),
    );
    const requiredProperties =
      (tool as any)?.configurations?.required?.filter((property: string) => {
        return propertiesToShow[property] !== undefined;
      }) || [];
    const newSchema = {
      ...(tool as any)?.configurations,
      properties: propertiesToShow,
      required: requiredProperties,
    } as unknown as RJSFSchema;
    setDynamicSchema(newSchema);
  }, [tool]);

  const [dynamicSchema, setDynamicSchema] = useState<RJSFSchema | null>(null);

  useEffect(() => {
    updateDynamicSchema();
  }, [tool, updateDynamicSchema]);

  const onAddConfigurationOverride = (configuration: string) => {
    console.log('onAddConfigurationOverride', configuration);
    const newConfig = { ...internalValue.current };
    newConfig[configuration] = null;
    mutateValue(newConfig);
    updateDynamicSchema();
  };

  const onDeleteConfigurationOverride = (configuration: string) => {
    console.log('onDeleteConfigurationOverride2', configuration);
    const newConfig = { ...internalValue.current };
    delete newConfig[configuration];
    mutateValue(newConfig);
    updateDynamicSchema();
  };

  const mutateValue = (newConfig: any) => {
    console.log('tryCallOnChange', newConfig);
    internalValue.current = newConfig;
    if (onChange) {
      onChange(newConfig);
    }
  };

  const templates = generateTemplates();

  return (
    <div className="flex flex-col gap-2">
      {isPending && <Skeleton className="flex-1 animate-pulse rounded" />}
      {isSuccess && tool && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <Select
              onValueChange={(newValue) => {
                onAddConfigurationOverride(newValue);
              }}
              value="default"
            >
              <SelectTrigger className="w-full p-4">
                <SelectValue>Select a configuration to override</SelectValue>
              </SelectTrigger>
              <SelectContent className="max-w-[720px] min-w-[180px] overflow-y-auto">
                {properties.map((property, index) => (
                  <SelectItem key={index} value={property.key}>
                    <div className="flex flex-col gap-1">
                      <span className="text-sm font-medium">
                        {property.name}
                      </span>
                      <span className="text-text-tertiary text-xs">
                        {property.description}
                      </span>
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {dynamicSchema && (
            <JsonForm
              className="py-1"
              formData={internalValue.current}
              liveValidate={true}
              noHtml5Validate={true}
              onChange={(e) => {
                console.log('onChange', e);
                if (e.errors.length === 0) {
                  mutateValue(e.formData);
                }
              }}
              schema={dynamicSchema}
              showErrorList={false}
              templates={{
                FieldTemplate: (props) => {
                  if (templates.FieldTemplate && props.id !== 'root') {
                    return (
                      <div className="border-divider flex w-full items-center gap-2 rounded-lg border p-2">
                        <div className="flex-grow">
                          <templates.FieldTemplate
                            classNames="w-full"
                            {...props}
                          />
                        </div>
                        <div className="flex-shrink-0 items-center justify-center px-2">
                          <Button
                            className="h-6 w-6 bg-red-500/10 text-red-500 transition-colors"
                            onClick={() => {
                              console.log('delete', props);
                              onDeleteConfigurationOverride(props.label);
                            }}
                            size="icon"
                            type="button"
                            variant="tertiary"
                          >
                            <Trash className="h-4 w-4" />
                          </Button>
                        </div>
                      </div>
                    );
                  } else if (templates.FieldTemplate) {
                    return <templates.FieldTemplate {...props} />;
                  }
                  return null;
                },
              }}
              uiSchema={{
                'ui:submitButtonOptions': {
                  norender: true,
                },
              }}
              validator={validator}
            />
          )}
        </div>
      )}
    </div>
  );
};
