import { FunctionKeyV2 } from '@shinkai_network/shinkai-node-state/v2/constants';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { TooltipProvider } from '@shinkai_network/shinkai-ui';
import { useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { debug } from '@tauri-apps/plugin-log';
import React, { useEffect, useRef } from 'react';
import {
  Navigate,
  Outlet,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router';

import AddAgentPage from '../components/agent/add-agent';
import EditAgentPage from '../components/agent/edit-agent';
import { ChatProvider } from '../components/chat/context/chat-context';
import { SetJobScopeProvider } from '../components/chat/context/set-job-scope-context';
import { StreamingProvider } from '../components/chat/context/streaming-context';
import { ToolsProvider as ToolsProviderChat } from '../components/chat/context/tools-context';
import { WalletsProvider } from '../components/crypto-wallet/context/wallets-context';
import DefaultLlmProviderUpdater from '../components/default-llm-provider/default-llm-provider-updater';
import {
  COMPLETION_DESTINATION,
  ONBOARDING_STEPS,
} from '../components/onboarding/constants';
import { PlaygroundProvider } from '../components/playground-tool/context/playground-context';
import { PromptSelectionProvider } from '../components/prompt/context/prompt-selection-context';
import { ToolsProvider } from '../components/tools/context/tools-context';
import ToolDetails from '../components/tools/tool-details';
import { VectorFolderSelectionProvider } from '../components/vector-fs/components/folder-selection-list';
import { VectorFsProvider } from '../components/vector-fs/context/vector-fs-context';
import VectorFs from '../components/vector-fs/vector-fs';
import SearchNodeFiles from '../components/vector-search/search-node-files';
import { useConfigDeepLink } from '../hooks/config-deeplink';
import {
  useShinkaiNodeGetDefaultModel,
  useShinkaiNodeIsRunningQuery,
  useShinkaiNodeSetOptionsMutation,
  useShinkaiNodeSpawnMutation,
} from '../lib/shinkai-node-manager/shinkai-node-manager-client';
import { useShinkaiNodeEventsToast } from '../lib/shinkai-node-manager/shinkai-node-manager-hooks';
import { ShinkaiNodeRunningOverlay } from '../lib/shinkai-node-overlay';
import AddAIPage from '../pages/add-ai';
import AgentsPage from '../pages/agents';
import AIModelInstallation from '../pages/ai-model-installation';
import AIsPage from '../pages/ais';
import AnalyticsPage from '../pages/analytics';
import AnalyticsSettingsPage from '../pages/analytics-settings';
import AppearancePage from '../pages/appearance';
import ChatConversation from '../pages/chat/chat-conversation';
import ChatLayout from '../pages/chat/layout';
import CreateTaskPage from '../pages/create-task';
import { CreateToolPage } from '../pages/create-tool';
import CryptoWalletPage from '../pages/crypto-wallet';
import EditTaskPage from '../pages/edit-task';
import EditToolPage from '../pages/edit-tool';
import { ExportConnection } from '../pages/export-connection';
import { GalxeValidation } from '../pages/galxe-validation';
import HomePage from '../pages/home';
import InternetAccessPage from '../pages/internet-access';
import MainLayout from '../pages/layout/main-layout';
import OnboardingLayout from '../pages/layout/onboarding-layout';
import SettingsLayout from '../pages/layout/settings-layout';
import { McpRegistryPage } from '../pages/mcp';
import { NetworkAgentPage } from '../pages/network-agents';
import { PromptLibrary } from '../pages/prompt-library';
import { PublicKeys } from '../pages/public-keys';
import QuickConnectionPage from '../pages/quick-connection';
import RestoreConnectionPage from '../pages/restore-connection';
import SettingsPage from '../pages/settings';
import ShortcutsPage from '../pages/shortcuts';
import { TaskLogs } from '../pages/task-logs';
import { Tasks } from '../pages/tasks';
import TermsAndConditionsPage, {
  LogoTapProvider,
} from '../pages/terms-conditions';
import ToolFeedbackPrompt from '../pages/tool-feedback';
import { ToolsPage } from '../pages/tools-homepage';
import { useAuth } from '../store/auth';
import { useSettings } from '../store/settings';
import { useShinkaiNodeManager } from '../store/shinkai-node-manager';
import useAppHotkeys from '../utils/use-app-hotkeys';

const skipOnboardingRoutes = ['/quick-connection', '/restore', '/connect-qr'];

const ProtectedRoute = ({ children }: { children: React.ReactNode }) => {
  const auth = useAuth((state) => state.auth);
  const shinkaiNodeOptions = useShinkaiNodeManager(
    (state) => state.shinkaiNodeOptions,
  );
  useShinkaiNodeEventsToast();
  const isInUse = useShinkaiNodeManager((state) => state.isInUse);
  const autoStartShinkaiNodeTried = useRef<boolean>(false);
  const { data: shinkaiNodeIsRunning } = useShinkaiNodeIsRunningQuery({
    refetchInterval: 1000,
  });
  const { mutateAsync: shinkaiNodeSetOptions } =
    useShinkaiNodeSetOptionsMutation();
  const { mutateAsync: shinkaiNodeSpawn } = useShinkaiNodeSpawnMutation();
  const location = useLocation();

  const isConnectionRoute = skipOnboardingRoutes.some(
    (route) =>
      location.pathname === route || location.pathname.startsWith(route + '/'),
  );

  /*
    All this auto start code is a workaround while we implement a way to synchronize the app state between browser and tauri
    Node auto start process probably should be in rust side
  */
  useEffect(() => {
    void debug(
      `initializing autoStartShinkaiNodeTried.current:${autoStartShinkaiNodeTried.current} isInUse:${isInUse} shinkaiNodeIsRunning:${shinkaiNodeIsRunning}`,
    );
    if (
      !autoStartShinkaiNodeTried.current &&
      isInUse &&
      !shinkaiNodeIsRunning
    ) {
      autoStartShinkaiNodeTried.current = true;
      void Promise.resolve().then(async () => {
        if (shinkaiNodeOptions) {
          await shinkaiNodeSetOptions(shinkaiNodeOptions);
        }
        await shinkaiNodeSpawn();
      });
    }
  }, [
    auth,
    shinkaiNodeSpawn,
    autoStartShinkaiNodeTried,
    shinkaiNodeIsRunning,
    shinkaiNodeOptions,
    shinkaiNodeSetOptions,
    isInUse,
  ]);

  if (!auth && !isConnectionRoute) {
    return <Navigate replace to={'/terms-conditions'} />;
  }

  return <ShinkaiNodeRunningOverlay>{children}</ShinkaiNodeRunningOverlay>;
};

const useGlobalAppShortcuts = () => {
  const navigate = useNavigate();
  useEffect(() => {
    const unlisten = listen('create-chat', () => {
      void navigate('/home');
    });

    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);
};

export const useDefaultAgentByDefault = () => {
  const auth = useAuth((state) => state.auth);
  const defaultAgentId = useSettings((state) => state.defaultAgentId);
  const setDefaultAgentId = useSettings((state) => state.setDefaultAgentId);
  const { data: defaultModel } = useShinkaiNodeGetDefaultModel();

  const { llmProviders, isSuccess } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  useEffect(() => {
    if (isSuccess && llmProviders?.length && !defaultAgentId) {
      const defaultAgent = llmProviders.find(
        (provider) =>
          provider.model.toLowerCase() === defaultModel?.toLowerCase(),
      );
      if (defaultAgent) {
        setDefaultAgentId(defaultAgent.id);
      } else {
        setDefaultAgentId(llmProviders[0].id);
      }
    }
  }, [
    llmProviders,
    isSuccess,
    setDefaultAgentId,
    defaultAgentId,
    defaultModel,
  ]);

  return null;
};

const OnboardingGuard = ({ children }: { children: React.ReactNode }) => {
  const auth = useAuth((state) => state.auth);
  const isStepCompleted = useSettings((state) => state.isStepCompleted);
  const getNextStep = useSettings((state) => state.getNextStep);
  const isOnboardingComplete = useSettings(
    (state) => state.isOnboardingComplete,
  );
  const getStepByPath = useSettings((state) => state.getStepByPath);
  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    if (skipOnboardingRoutes.includes(location.pathname)) {
      return;
    }

    if (!auth && location.pathname !== '/terms-conditions') {
      void navigate('/terms-conditions');
      return;
    }

    if (isOnboardingComplete() && !!auth) {
      void navigate(COMPLETION_DESTINATION);
      return;
    }

    const nextIncompleteStep = getNextStep();
    if (!nextIncompleteStep) return;

    const currentStep = getStepByPath(location.pathname);
    const isRootPath = [COMPLETION_DESTINATION, '/'].includes(
      location.pathname,
    );
    const isValidPath = location.pathname === nextIncompleteStep.path;

    if (
      (currentStep && isStepCompleted(currentStep.id)) ||
      isRootPath ||
      !isValidPath
    ) {
      void navigate(nextIncompleteStep.path);
    }
  }, [
    auth,
    isStepCompleted,
    getNextStep,
    getStepByPath,
    isOnboardingComplete,
    location.pathname,
    navigate,
  ]);

  return (
    <LogoTapProvider>
      <OnboardingLayout>{children}</OnboardingLayout>
    </LogoTapProvider>
  );
};

const useNavigateToPathFromSpotlight = () => {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  useEffect(() => {
    const unlisten = listen('navigate-to-path', (event) => {
      const inboxId = event.payload as string;
      void queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION,
          {
            inboxId: decodeURIComponent(inboxId),
          },
        ],
      });
      void navigate(inboxId);
    });

    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [navigate, queryClient]);
};

const AppRoutes = () => {
  useAppHotkeys();
  useGlobalAppShortcuts();
  useDefaultAgentByDefault();
  useConfigDeepLink();
  useNavigateToPathFromSpotlight();

  const defaultAgentId = useSettings((state) => state.defaultAgentId);

  return (
    <>
      {/* Use the DefaultLlmProviderUpdater component to update the default LLM provider */}
      {defaultAgentId && (
        <DefaultLlmProviderUpdater defaultAgentId={defaultAgentId} />
      )}
      <Routes>
        <Route element={<MainLayout />}>
          <Route
            element={
              <OnboardingGuard>
                <Outlet />
              </OnboardingGuard>
            }
          >
            <Route
              element={<TermsAndConditionsPage />}
              path={'terms-conditions'}
            />
            <Route element={<AnalyticsPage />} path={'analytics'} />
            <Route
              element={<QuickConnectionPage />}
              path={'quick-connection'}
            />
            <Route element={<RestoreConnectionPage />} path={'restore'} />
            {/* <Route element={<ConnectMethodQrCodePage />} path={'connect-qr'} /> */}
          </Route>
          <Route
            element={
              <ProtectedRoute>
                <TooltipProvider delayDuration={0}>
                  <ChatProvider>
                    <SetJobScopeProvider>
                      <PromptSelectionProvider>
                        <StreamingProvider>
                          <ToolsProviderChat>
                            <Outlet />
                          </ToolsProviderChat>
                        </StreamingProvider>
                      </PromptSelectionProvider>
                    </SetJobScopeProvider>
                  </ChatProvider>
                </TooltipProvider>
              </ProtectedRoute>
            }
          >
            <Route element={<HomePage />} path={'home'} />
          </Route>
          <Route
            element={
              <ProtectedRoute>
                <TooltipProvider delayDuration={0}>
                  <ChatProvider>
                    <SetJobScopeProvider>
                      <PromptSelectionProvider>
                        <StreamingProvider>
                          <ToolsProviderChat>
                            <ChatLayout />
                          </ToolsProviderChat>
                        </StreamingProvider>
                      </PromptSelectionProvider>
                    </SetJobScopeProvider>
                  </ChatProvider>
                </TooltipProvider>
              </ProtectedRoute>
            }
            path="inboxes"
          >
            {/* <Route element={<ChatConversation />} index /> */}
            <Route element={<ChatConversation />} path=":inboxId" />
          </Route>
          <Route
            element={
              <ProtectedRoute>
                <VectorFsProvider>
                  <Outlet />
                </VectorFsProvider>
              </ProtectedRoute>
            }
          >
            <Route element={<VectorFs />} path="vector-fs" />
            <Route
              element={
                <VectorFolderSelectionProvider>
                  <SearchNodeFiles />
                </VectorFolderSelectionProvider>
              }
              path="vector-search"
            />
          </Route>

          <Route
            element={
              <ProtectedRoute>
                <Outlet />
              </ProtectedRoute>
            }
          >
            <Route element={<AIModelInstallation />} path="install-ai-models" />
            <Route element={<AIsPage />} path="ais" />
            <Route element={<AgentsPage />} path="agents" />
            <Route element={<AddAIPage />} path="add-ai" />
            <Route
              element={
                <VectorFsProvider>
                  <SetJobScopeProvider>
                    <AddAgentPage />
                  </SetJobScopeProvider>
                </VectorFsProvider>
              }
              path="add-agent"
            />
            <Route
              element={
                <VectorFsProvider>
                  <SetJobScopeProvider>
                    <EditAgentPage />
                  </SetJobScopeProvider>
                </VectorFsProvider>
              }
              path="/agents/edit/:agentId"
            />
          </Route>
          <Route
            element={
              <ProtectedRoute>
                <ToolsProvider>
                  <ChatProvider>
                    <Outlet />
                  </ChatProvider>
                </ToolsProvider>
              </ProtectedRoute>
            }
            path={'tools'}
          >
            <Route element={<ToolsPage />} index />
            <Route
              element={
                <PlaygroundProvider>
                  <ToolDetails />
                </PlaygroundProvider>
              }
              path={':toolKey'}
            />
            <Route
              element={
                <PlaygroundProvider>
                  <CreateToolPage />
                </PlaygroundProvider>
              }
              path={'create'}
            />
            <Route
              element={
                <PlaygroundProvider>
                  <ToolFeedbackPrompt />
                </PlaygroundProvider>
              }
              path={'tool-feedback/:inboxId'}
            />
            <Route
              element={
                <PlaygroundProvider>
                  <EditToolPage />
                </PlaygroundProvider>
              }
              path={'edit/:toolRouterKey'}
            />
          </Route>
          <Route
            element={
              <ProtectedRoute>
                <Outlet />
              </ProtectedRoute>
            }
          >
            <Route element={<McpRegistryPage />} path={'mcp'} />
            <Route element={<NetworkAgentPage />} path={'network-ai-agents'} />
          </Route>
          <Route
            element={
              <ProtectedRoute>
                <Outlet />
              </ProtectedRoute>
            }
            path={'tasks'}
          >
            <Route element={<Tasks />} index />
            <Route element={<TaskLogs />} path={':taskId'} />
            <Route element={<CreateTaskPage />} path={'create'} />
            <Route element={<EditTaskPage />} path={'edit/:taskId'} />
          </Route>
          <Route
            element={
              <ProtectedRoute>
                <SettingsLayout />
              </ProtectedRoute>
            }
            path={'settings'}
          >
            <Route element={<SettingsPage />} index />
            <Route element={<ExportConnection />} path={'export-connection'} />
            <Route element={<PublicKeys />} path={'public-keys'} />
            <Route
              element={<AnalyticsSettingsPage />}
              path={'analytics-settings'}
            />
            <Route element={<AppearancePage />} path={'appearance'} />
            <Route element={<PromptLibrary />} path={'prompt-library'} />
            <Route element={<GalxeValidation />} path={'galxe-validation'} />
            <Route
              element={
                <WalletsProvider>
                  <CryptoWalletPage />
                </WalletsProvider>
              }
              path={'crypto-wallet'}
            />
            <Route element={<InternetAccessPage />} path={'remote-access'} />
            <Route element={<ShortcutsPage />} path={'shortcuts'} />
          </Route>
        </Route>
        <Route
          element={<Navigate replace to={ONBOARDING_STEPS[0].path} />}
          path="*"
        />
      </Routes>
    </>
  );
};
export default AppRoutes;
