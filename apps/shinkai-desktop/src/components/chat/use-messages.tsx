import { useEffect, useRef, useState } from 'react';

import { useScrollToBottom } from '../../hooks/use-scroll-bottom';

type ChatStatus = 'submitted' | 'streaming' | 'ready' | 'error';

export function useMessages({ status }: { status: ChatStatus }) {
  const scrollState = useScrollToBottom();
  const [hasSentMessage, setHasSentMessage] = useState(false);
  const prevStatusRef = useRef<ChatStatus>(status);

  useEffect(() => {
    const prevStatus = prevStatusRef.current;

    if (status === 'submitted' && prevStatus !== 'submitted') {
      setHasSentMessage(true);
      scrollState.setAutoScroll(true);
      scrollState.scrollDomToBottom();
    }

    if (status === 'ready' && prevStatus !== 'ready') {
      setHasSentMessage(false);
    }

    prevStatusRef.current = status;
  }, [status, scrollState]);

  return {
    ...scrollState,
    hasSentMessage,
  };
}
