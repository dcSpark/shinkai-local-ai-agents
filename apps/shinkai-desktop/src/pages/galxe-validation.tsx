import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { QuestNames } from '@shinkai_network/shinkai-message-ts/api/quests/types';
import { useUpdateQuestsStatus } from '@shinkai_network/shinkai-node-state/v2/mutations/updateQuestsStatus/useUpdateQuestsStatus';
import { useGetQuestsStatus } from '@shinkai_network/shinkai-node-state/v2/queries/getQuestsStatus/useGetQuestsStatus';
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Skeleton,
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@shinkai_network/shinkai-ui';
import { Check, CircleIcon, Info, RefreshCw } from 'lucide-react';
import { useMemo } from 'react';

import { useAuth } from '../store/auth';
import { SimpleLayout } from './layout/simple-layout';

const questStatusInfoMap: Record<
  QuestNames,
  { name: string; description: string }
> = {
  [QuestNames.InstalledApp]: {
    name: 'Try Shinkai Desktop',
    description: 'Download and Install Shinkai Desktop.',
  },
  [QuestNames.CreateIdentity]: {
    name: 'Create Your Shinkai Identity',
    description:
      'Create an on-chain Shinkai Identity! This is require to participate in the decentralized peer to peer network.',
  },
  [QuestNames.DownloadFromStore]: {
    name: 'Download Your First Tool',
    description: 'Download your first tool from the Shinkai AI store.',
  },
  [QuestNames.ComeBack2Days]: {
    name: 'Return to Shinkai in 2 different days',
    description:
      'Return to Shinkai in 2 different days. Remember to create new chats each day!',
  },
  [QuestNames.ComeBack4Days]: {
    name: 'Return to Shinkai in 4 different days',
    description:
      'Return to Shinkai in 4 different days. Remember to create new chats each day!',
  },
  [QuestNames.ComeBack7Days]: {
    name: 'Return to Shinkai in 7 different days',
    description:
      'Return to Shinkai in 7 different days. Remember to create new chats each day!',
  },
  [QuestNames.CreateTool]: {
    name: 'Create Your First Tool',
    description:
      'Create and publish your first tool on Shinkai. Share your creativity with the community.',
  },
  [QuestNames.SubmitAndGetApprovalForTool]: {
    name: 'Submit and Get Approval for 1 Tool',
    description:
      'Submit your tool for review and get it approved by the Shinkai team.',
  },
  [QuestNames.SubmitAndGetApprovalFor2Tool]: {
    name: 'Submit and Get Approval for 2 Tools',
    description:
      'Submit your tool for review and get it approved by the Shinkai team.',
  },
  [QuestNames.SubmitAndGetApprovalFor3Tool]: {
    name: 'Submit and Get Approval for 3 Tools',
    description:
      'Submit your tool for review and get it approved by the Shinkai team.',
  },
  [QuestNames.FeaturedInRanking]: {
    name: 'Featured in Ranking',
    description: 'Get featured in the Shinkai AI Store',
  },
  [QuestNames.WriteHonestReview]: {
    name: 'Write a Review',
    description:
      'Go to Shinkai Store and write a detailed and honest review about your experience with a Shinkai tool.',
  },
  [QuestNames.Write5HonestReview]: {
    name: 'Write 5 Reviews',
    description:
      'Go to Shinkai Store and write 5 detailed and honest reviews about your experience with Shinkai tools.',
  },
  [QuestNames.Write10HonestReview]: {
    name: 'Write 10 Reviews',
    description:
      'Go to Shinkai Store and write 10 detailed and honest reviews about your experience with Shinkai tools.',
  },
  [QuestNames.UseRAG3Days]: {
    name: 'Chat with files 3 different days',
    description:
      'Experience the power of Retrieval-Augmented Generation (RAG) while chat with your files in 3 different days.',
  },
};

export const GalxeValidation = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const { data, isPending, isSuccess } = useGetQuestsStatus({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { mutate: updateQuestsStatus, isPending: isQuestsStatusUpdating } =
    useUpdateQuestsStatus();

  const quests = useMemo(() => {
    return data?.data?.quests.map((quest) => ({
      key: quest.name,
      name: questStatusInfoMap[quest.name].name,
      description: questStatusInfoMap[quest.name].description,
      progress: quest.status ? 100 : 0,
    }));
  }, [data]);

  return (
    <SimpleLayout
      classname="container"
      headerRightElement={
        <Button
          disabled={isQuestsStatusUpdating}
          isLoading={isQuestsStatusUpdating}
          onClick={() =>
            updateQuestsStatus({
              nodeAddress: auth?.node_address ?? '',
              token: auth?.api_v2_key ?? '',
            })
          }
          size="xs"
        >
          {isQuestsStatusUpdating ? null : <RefreshCw className="h-4 w-4" />}
          {isQuestsStatusUpdating ? 'Syncing...' : 'Sync Quests'}
        </Button>
      }
      title={t('galxe.label')}
    >
      <div className="space-y-4 py-2 pb-10">
        <Alert className="rounded-xs border-none bg-cyan-900/20 p-3.5 shadow-none">
          <Info className="mt-1 size-4 text-cyan-400" />
          <AlertTitle className="text-sm">
            Setup Your Shinkai Identity
          </AlertTitle>
          <AlertDescription className="text-text-secondary text-xs">
            Before you can sync and track your quest progress, you&apos;ll need
            to set up your Shinkai identity.
          </AlertDescription>
        </Alert>
        {isPending &&
          Array.from({ length: 10 }).map((_, index) => (
            <Skeleton className="h-20 w-full" key={index} />
          ))}
        {isSuccess &&
          quests?.map((quest, index) => (
            <Card className="m-0 mb-4 p-3 px-4" key={index}>
              <CardHeader className="flex flex-row items-start gap-2.5 space-y-0 p-0">
                <span className="pt-1">
                  {quest.progress === 100 ? (
                    <div className="flex size-5 items-center justify-center rounded-full bg-green-600 p-0.5">
                      <Check className="text-white" />
                    </div>
                  ) : (
                    <CircleIcon className="size-5 text-green-400" />
                  )}
                </span>
                <div className="m-0 flex flex-1 flex-col gap-1">
                  <CardTitle className="flex items-center gap-2 p-0 text-base font-semibold">
                    {quest.name}
                  </CardTitle>
                  <CardDescription className="text-text-secondary p-0 text-sm">
                    {quest.description}
                  </CardDescription>
                </div>
              </CardHeader>
            </Card>
          ))}
      </div>
    </SimpleLayout>
  );
};
