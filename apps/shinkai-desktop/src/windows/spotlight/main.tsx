import './globals.css';

import { I18nProvider } from '@shinkai_network/shinkai-i18n';
import { QueryProvider } from '@shinkai_network/shinkai-node-state';
import { Toaster, TooltipProvider } from '@shinkai_network/shinkai-ui';
import { QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary } from 'react-error-boundary';
import { BrowserRouter as Router } from 'react-router';

import { ChatProvider } from '../../components/chat/context/chat-context';
import { StreamingProvider } from '../../components/chat/context/streaming-context';
import { ToolsProvider } from '../../components/chat/context/tools-context';
import FullPageErrorFallback from '../../components/error-boundary';
import { shinkaiNodeQueryClient } from '../../lib/shinkai-node-manager/shinkai-node-manager-client';
import { useShinkaiNodeEventsToast } from '../../lib/shinkai-node-manager/shinkai-node-manager-hooks';
import { ShinkaiNodeRunningOverlay } from '../../lib/shinkai-node-overlay';
import { useSyncStorageSecondary } from '../../store/sync-utils';
import QuickAsk from './components/quick-ask';
import { QuickAskProvider } from './context/quick-ask';

const App = () => {
  useSyncStorageSecondary();
  useShinkaiNodeEventsToast();

  return (
    <I18nProvider>
      <ErrorBoundary FallbackComponent={FullPageErrorFallback}>
        <QuickAskProvider>
          <StreamingProvider>
            <ToolsProvider>
              <ShinkaiNodeRunningOverlay>
                <ChatProvider>
                  <TooltipProvider delayDuration={0}>
                    <Router>
                      {/*<Routes>*/}
                      <QuickAsk />
                      {/*</Routes>*/}
                    </Router>
                    <Toaster />
                  </TooltipProvider>
                </ChatProvider>
              </ShinkaiNodeRunningOverlay>
            </ToolsProvider>
          </StreamingProvider>
        </QuickAskProvider>
      </ErrorBoundary>
    </I18nProvider>
  );
};

ReactDOM.createRoot(document.querySelector('#root') as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={shinkaiNodeQueryClient}>
      <QueryProvider>
        <App />
      </QueryProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
