import { useCallback, useEffect, useRef, useState } from 'react';

import { useChatStore } from '../components/chat/context/chat-context';

const SCROLL_THRESHOLD = 100;

export function useScrollToBottom() {
  const containerRef = useRef<HTMLDivElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const previousScrollHeightRef = useRef<number>(0);
  const [isAtBottom, setIsAtBottom] = useState(true);

  const scrollBehavior = useChatStore((state) => state.scrollBehavior);
  const setScrollBehavior = useChatStore((state) => state.setScrollBehavior);
  const autoScroll = useChatStore((state) => state.autoScroll);
  const setAutoScroll = useChatStore((state) => state.setAutoScroll);

  const checkIsAtBottom = useCallback(() => {
    if (!containerRef.current) return true;
    const { scrollTop, scrollHeight, clientHeight } = containerRef.current;
    return scrollTop + clientHeight >= scrollHeight - SCROLL_THRESHOLD;
  }, []);

  const handleScroll = useCallback(() => {
    if (!containerRef.current) return;

    const container = containerRef.current;
    const { scrollTop, scrollHeight, clientHeight } = container;
    const isNearBottom =
      scrollTop + clientHeight >= scrollHeight - SCROLL_THRESHOLD;

    setIsAtBottom(isNearBottom);
    if (isNearBottom) {
      setAutoScroll(true);
    }
  }, [setAutoScroll]);

  const scrollDomToBottom = useCallback(() => {
    const container = containerRef.current;
    if (container) {
      requestAnimationFrame(() => {
        container.scrollTo(0, container.scrollHeight);
        setIsAtBottom(true);
      });
    }
  }, []);

  const scrollToBottom = useCallback(
    (behavior: ScrollBehavior = 'smooth') => {
      setScrollBehavior(behavior);
    },
    [setScrollBehavior],
  );

  const storeScrollHeight = useCallback(() => {
    if (containerRef.current) {
      previousScrollHeightRef.current = containerRef.current.scrollHeight;
    }
  }, []);

  const preserveScrollPosition = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;

    const currentHeight = container.scrollHeight;
    const previousHeight = previousScrollHeightRef.current;
    const heightDiff = currentHeight - previousHeight;

    if (heightDiff > 0) {
      container.scrollTop = container.scrollTop + heightDiff;
    }

    previousScrollHeightRef.current = currentHeight;
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    container.addEventListener('scroll', handleScroll, { passive: true });
    handleScroll();

    return () => {
      container.removeEventListener('scroll', handleScroll);
    };
  }, [handleScroll]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const mutationObserver = new MutationObserver(() => {
      if (autoScroll) {
        requestAnimationFrame(() => {
          scrollDomToBottom();
        });
      }
    });

    mutationObserver.observe(container, {
      childList: true,
      subtree: true,
      characterData: true, // For text content changes during streaming
    });

    return () => {
      mutationObserver.disconnect();
    };
  }, [autoScroll, scrollDomToBottom]);

  useEffect(() => {
    if (scrollBehavior && containerRef.current) {
      const container = containerRef.current;
      container.scrollTo({
        top: container.scrollHeight,
        behavior: scrollBehavior,
      });
      setScrollBehavior(false);
      setAutoScroll(true);
    }
  }, [scrollBehavior, setScrollBehavior, setAutoScroll]);

  return {
    containerRef,
    endRef,
    isAtBottom,
    autoScroll,
    setAutoScroll,
    scrollToBottom,
    scrollDomToBottom,
    storeScrollHeight,
    preserveScrollPosition,
    checkIsAtBottom,
    previousScrollHeightRef,
  };
}
