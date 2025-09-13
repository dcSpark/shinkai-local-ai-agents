import { useStartEmbeddingMigration } from '@shinkai_network/shinkai-node-state/v2/mutations/startEmbeddingMigration/useStartEmbeddingMigration';
import { useGetEmbeddingMigrationStatus } from '@shinkai_network/shinkai-node-state/v2/queries/getEmbeddingMigrationStatus/useGetEmbeddingMigrationStatus';
import { useScanOllamaModels } from '@shinkai_network/shinkai-node-state/v2/queries/scanOllamaModels/useScanOllamaModels';
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Label,
  RadioGroup,
  RadioGroupItem,
} from '@shinkai_network/shinkai-ui';
import { Edit3Icon } from 'lucide-react';
import React, { useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { useAuth } from '../../store/auth';

const EmbeddingModelSelectionDialog = () => {
  const auth = useAuth((state) => state.auth);
  const [isOpen, setIsOpen] = useState(false);

  const [selectedEmbeddingModel, setSelectedEmbeddingModel] = useState('');

  const { data: availableOllamaModels } = useScanOllamaModels(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    { enabled: !!auth },
  );

  const supportedEmbeddingModels = useMemo(
    () => [
      'snowflake-arctic-embed:xs',
      'embeddinggemma:300m',
      'jina/jina-embeddings-v2-base-es:latest',
    ],
    [],
  );

  const availableEmbeddingModels = useMemo(() => {
    if (!availableOllamaModels) return [];

    return availableOllamaModels
      .filter((model) => supportedEmbeddingModels.includes(model.model))
      .map((model) => ({
        value: model.model,
        label: model.model,
      }));
  }, [availableOllamaModels, supportedEmbeddingModels]);

  const { data: embeddingMigrationStatus } = useGetEmbeddingMigrationStatus(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    {
      enabled: !!auth,
    },
  );

  const currentEmbeddingModel =
    embeddingMigrationStatus?.current_embedding_model;

  const {
    mutateAsync: startEmbeddingMigration,
    isPending: isMigratingEmbedding,
  } = useStartEmbeddingMigration({
    onSuccess: () => {
      setIsOpen(false);
      setSelectedEmbeddingModel(selectedEmbeddingModel);
      toast.success('Embedding model updated successfully');
    },
    onError: (error) => {
      setIsOpen(false);
      toast.error('Failed to update embedding model', {
        description: error.message,
      });
    },
  });

  const confirmMigration = async () => {
    if (!selectedEmbeddingModel) return;

    await startEmbeddingMigration({
      nodeAddress: auth!.node_address,
      token: auth!.api_v2_key,
      force: true,
      embedding_model: selectedEmbeddingModel,
    });
  };

  const cancelMigration = () => {
    setIsOpen(false);
    setSelectedEmbeddingModel('');
  };

  useEffect(() => {
    if (currentEmbeddingModel) {
      setSelectedEmbeddingModel(currentEmbeddingModel);
    }
  }, [currentEmbeddingModel]);

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogTrigger asChild>
        <Button
          onClick={() => setIsOpen(true)}
          disabled={isMigratingEmbedding}
          variant="outline"
          size="sm"
          className="min-w-[100px] gap-2 rounded-md"
        >
          <span className="sr-only"> Select Embedding Model</span>
          <span className="text-text-default text-sm font-normal">
            {currentEmbeddingModel}
          </span>
          <Edit3Icon className="h-4 w-4" />
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-lg" showCloseButton>
        <DialogHeader>
          <DialogTitle className="text-lg font-semibold">
            Update Embedding Model
          </DialogTitle>
          <DialogDescription className="text-text-secondary text-sm">
            Choose the embedding model that best fits your needs. This process
            will re-process existing vectorized data.
          </DialogDescription>

          <div className="bg-bg-tertiary border-divider mt-4 rounded-lg border p-4">
            <h4 className="text-text-default mb-3 text-sm font-medium">
              This process will:
            </h4>
            <ul className="text-text-secondary space-y-2 text-sm">
              <li className="flex items-start gap-2">
                <span className="text-text-tertiary mt-1.5 h-1.5 w-1.5 rounded-full bg-current"></span>
                Update your embedding model configuration
              </li>
              <li className="flex items-start gap-2">
                <span className="text-text-tertiary mt-1.5 h-1.5 w-1.5 rounded-full bg-current"></span>
                Re-process existing vectorized data (this may take some time)
              </li>
              <li className="flex items-start gap-2">
                <span className="text-text-tertiary mt-1.5 h-1.5 w-1.5 rounded-full bg-current"></span>
                Temporarily affect search functionality during migration
              </li>
            </ul>
          </div>
        </DialogHeader>

        <div className="mt-6">
          <RadioGroup
            value={selectedEmbeddingModel}
            onValueChange={setSelectedEmbeddingModel}
            className="gap-1.5"
          >
            {availableEmbeddingModels.map((model) => (
              <div
                key={model.value}
                className={`border-divider flex items-center gap-0 rounded-lg border px-4 ${
                  model.value === currentEmbeddingModel
                    ? 'ring-brand/20 ring-2'
                    : ''
                }`}
              >
                <RadioGroupItem value={model.value} id={model.value} />
                <Label
                  htmlFor={model.value}
                  className="flex-1 cursor-pointer font-medium"
                >
                  <div className="flex items-center gap-3 p-4">
                    <div className="flex-1">
                      <p className="flex items-center gap-2 text-sm font-medium">
                        {model.value}
                        {model.value === 'embeddinggemma:300m' && (
                          <Badge variant="inputAdornment">Recommended</Badge>
                        )}
                        {model.value === currentEmbeddingModel && (
                          <Badge variant="inputAdornment">Current</Badge>
                        )}
                      </p>
                      <p className="text-text-secondary mt-1 text-xs">
                        {model.value.includes('snowflake') &&
                          'High-performance embedding model optimized for speed and efficiency'}
                        {model.value.includes('gemma') &&
                          "Google's Gemma-based embedding model with excellent accuracy"}
                        {model.value.includes('jina') &&
                          'Multilingual embedding model supporting 100+ languages'}
                      </p>
                    </div>
                  </div>
                </Label>
              </div>
            ))}
          </RadioGroup>
        </div>

        <DialogFooter className="mt-8 flex flex-col gap-1">
          <Button
            onClick={cancelMigration}
            disabled={isMigratingEmbedding}
            variant="outline"
            size="sm"
            className="w-auto min-w-[110px]"
          >
            Cancel
          </Button>
          <Button
            onClick={confirmMigration}
            disabled={!selectedEmbeddingModel || isMigratingEmbedding}
            className="w-auto min-w-[110px]"
            size="sm"
            variant="default"
            isLoading={isMigratingEmbedding}
          >
            {isMigratingEmbedding ? 'Updating...' : 'Update'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default EmbeddingModelSelectionDialog;
