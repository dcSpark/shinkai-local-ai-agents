import { useEffect, useState } from 'react';

import { useScrollToBottom } from '../../hooks/use-scroll-bottom';

type ChatStatus = 'submitted' | 'streaming' | 'ready' | 'error';

export function useMessages({ status }: { status: ChatStatus }) {
  const scrollState = useScrollToBottom();
  const [hasSentMessage, setHasSentMessage] = useState(false);

  useEffect(() => {
    if (status === 'submitted') {
      setHasSentMessage(true);
    }
  }, [status]);

  return {
    ...scrollState,
    hasSentMessage,
  };
}
