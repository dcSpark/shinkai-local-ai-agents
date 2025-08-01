import { BackgroundBeams, buttonVariants } from '@shinkai_network/shinkai-ui';

import { useScrollRestoration } from '@shinkai_network/shinkai-ui/hooks';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { ArrowRight } from 'lucide-react';
import { FeedbackModal } from '../components/feedback/feedback-modal';

import { useToolsStore } from '../components/tools/context/tools-context';
import { ToolCollection } from '../components/tools/tool-collection';
import { useViewportStore } from '../store/viewport';
import { SHINKAI_STORE_URL } from '../utils/store';

export function ToolsPage() {
  const toolHomepageScrollPositionRef = useToolsStore(
    (state) => state.toolHomepageScrollPositionRef,
  );

  const mainLayoutContainerRef = useViewportStore(
    (state) => state.mainLayoutContainerRef,
  );

  useScrollRestoration({
    key: 'tools',
    containerRef: mainLayoutContainerRef,
    scrollTopStateRef: toolHomepageScrollPositionRef,
  });

  return (
    <div className="container">
      <div className="flex flex-col gap-6 pb-10">
        <ToolCollection />

        <div className="bg-official-gray-1000 relative rounded-lg">
          <div className="relative z-[1] mx-auto flex flex-col items-center gap-8 p-10 text-center">
            <div className="flex flex-col gap-2">
              <h3 className="font-clash max-w-xl text-2xl font-semibold tracking-normal">
                Discover More Tools
              </h3>
              <p className="text-text-secondary max-w-xl text-base leading-relaxed tracking-tight">
                Explore and install tools from our App Store to boost your
                productivity and automate your workflow.
              </p>
            </div>
            <div className="isolate flex flex-row gap-4">
              <div className="flex items-center gap-3">
                <a
                  className={cn(buttonVariants({ size: 'sm' }), 'gap-4 px-4')}
                  href={SHINKAI_STORE_URL}
                  rel="noreferrer"
                  target="_blank"
                >
                  Visit App Store <ArrowRight className="h-4 w-4" />
                </a>
                <FeedbackModal buttonProps={{ rounded: 'full' }} />
              </div>
            </div>
          </div>
          <BackgroundBeams />
        </div>
      </div>
    </div>
  );
}
