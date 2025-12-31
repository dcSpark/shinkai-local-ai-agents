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

  // Check if we're near the bottom of the container
  const checkIsAtBottom = useCallback(() => {
    if (!containerRef.current) return true;
    const { scrollTop, scrollHeight, clientHeight } = containerRef.current;
    return scrollTop + clientHeight >= scrollHeight - SCROLL_THRESHOLD;
  }, []);

  // Handle scroll events
  const handleScroll = useCallback(() => {
    if (!containerRef.current) return;

    const container = containerRef.current;
    const { scrollTop, scrollHeight, clientHeight } = container;
    const isNearBottom =
      scrollTop + clientHeight >= scrollHeight - SCROLL_THRESHOLD;

    setIsAtBottom(isNearBottom);
    setAutoScroll(isNearBottom);
    previousScrollHeightRef.current = scrollHeight;
  }, [setAutoScroll]);

  // Instant scroll to bottom (no animation)
  const scrollDomToBottom = useCallback(() => {
    const container = containerRef.current;
    if (container) {
      requestAnimationFrame(() => {
        setAutoScroll(true);
        container.scrollTo(0, container.scrollHeight);
      });
    }
  }, [setAutoScroll]);

  // Smooth scroll to bottom with behavior option
  const scrollToBottom = useCallback(
    (behavior: ScrollBehavior = 'smooth') => {
      setScrollBehavior(behavior);
    },
    [setScrollBehavior],
  );

  // Preserve scroll position when content is prepended (for pagination)
  const preserveScrollPosition = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;

    const currentHeight = container.scrollHeight;
    const previousHeight = previousScrollHeightRef.current;
    const heightDiff = currentHeight - previousHeight;

    if (heightDiff > 0 && !autoScroll) {
      container.scrollTop = container.scrollTop + heightDiff;
    }

    previousScrollHeightRef.current = currentHeight;
  }, [autoScroll]);

  // Set up scroll event listener
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    container.addEventListener('scroll', handleScroll, { passive: true });
    handleScroll(); // Check initial state

    return () => {
      container.removeEventListener('scroll', handleScroll);
    };
  }, [handleScroll]);

  // Set up resize and mutation observers for auto-scroll
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const resizeObserver = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        if (autoScroll) {
          scrollDomToBottom();
        }
        handleScroll();
      });
    });

    const mutationObserver = new MutationObserver(() => {
      requestAnimationFrame(() => {
        if (autoScroll) {
          scrollDomToBottom();
        }
        handleScroll();
      });
    });

    resizeObserver.observe(container);
    mutationObserver.observe(container, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['style', 'class', 'data-state'],
    });

    return () => {
      resizeObserver.disconnect();
      mutationObserver.disconnect();
    };
  }, [autoScroll, handleScroll, scrollDomToBottom]);

  // Handle scroll behavior changes (for smooth scrolling)
  useEffect(() => {
    if (scrollBehavior && containerRef.current) {
      const container = containerRef.current;
      container.scrollTo({
        top: container.scrollHeight,
        behavior: scrollBehavior,
      });
      setScrollBehavior(false);
    }
  }, [scrollBehavior, setScrollBehavior]);

  return {
    containerRef,
    endRef,
    isAtBottom,
    autoScroll,
    setAutoScroll,
    scrollToBottom,
    scrollDomToBottom,
    preserveScrollPosition,
    checkIsAtBottom,
    previousScrollHeightRef,
  };
}
