# Frontend Streaming Architecture

This document explains the frontend streaming architecture for real-time AI chat responses, detailing how streaming tokens are handled efficiently without causing performance issues.

## Overview

The streaming system uses a **hybrid approach** that separates ephemeral streaming state from persistent React Query cache to minimize re-renders and provide smooth streaming UX:

- **WebSocket** - Real-time token delivery from backend
- **Zustand Store** - Ephemeral streaming state (avoids expensive React Query re-renders)
- **React Query** - Persistent message data and caching

```
┌─────────────────┐      ┌─────────────────┐      ┌─────────────────┐
│   User sends    │──────│  HTTP API       │──────│   Backend       │
│   message       │      │  (mutation)     │      │   (Shinkai)     │
└─────────────────┘      └─────────────────┘      └─────────────────┘
                                                          │
                         WebSocket (streaming tokens)     │
                    ◄─────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
        ▼                       ▼                       ▼
┌───────────────┐     ┌─────────────────┐     ┌─────────────────┐
│ Zustand Store │     │ React Query     │     │ StreamingMessage│
│ (streaming)   │     │ (final data)    │     │ (renders)       │
└───────────────┘     └─────────────────┘     └─────────────────┘
```

## Key Design Decisions

### Why Separate Streaming Store from React Query?

If we updated React Query on every token (~20 tokens/second), the entire message list would re-render ~20 times per second, causing:

- Poor performance and UI jank
- Excessive re-renders of all message components
- Poor user experience during streaming

**Solution**: Use Zustand store for ephemeral streaming state, only update React Query once when streaming completes.

### Token Buffering Strategy

Tokens are buffered in refs and flushed at **~20 FPS** (50ms intervals) for smooth streaming:

- Prevents excessive state updates
- Balances smoothness with performance
- Human perception threshold: ~20 FPS is smooth enough

## Architecture Components

### 1. WebSocket Message Handler (`websocket-message.tsx`)

Manages WebSocket connection and processes incoming messages.

**Key Responsibilities:**

- Subscribe/unsubscribe to inbox topics via WebSocket
- Buffer incoming tokens for batched updates
- Detect message types (user, assistant, stream tokens, widgets)
- Finalize streaming and update React Query with final data

**Hooks:**

- `useWebSocketMessage` - Handles message streaming
- `useWebSocketTools` - Handles tool call widgets

**Token Buffering:**

```typescript
const FLUSH_INTERVAL_MS = 50; // 20 FPS

// Tokens are buffered in refs, not state
const tokenBufferRef = useRef('');
const reasoningBufferRef = useRef('');

// Flushed periodically to streaming store
if (!flushScheduledRef.current) {
  flushScheduledRef.current = true;
  flushTimeoutRef.current = setTimeout(() => {
    flushTimeoutRef.current = null;
    flushTokenBuffer();
  }, FLUSH_INTERVAL_MS);
}
```

### 2. Streaming Store (`context/streaming-context.tsx`)

A Zustand store that holds ephemeral streaming data separately from React Query.

**Store Structure:**

```typescript
type StreamingStore = {
  // Ephemeral streaming content (cleared after finalization)
  streams: Map<string, StreamingContent>;

  // Persistent reasoning durations (last 5 inboxes)
  reasoningDurations: Map<string, number>;

  // Methods...
  startStreaming: (inboxId: string) => void;
  appendContent: (inboxId: string, content: string) => void;
  appendReasoning: (inboxId: string, reasoning: string) => void;
  endStreaming: (inboxId: string) => void;
  clearInbox: (inboxId: string) => void;
  saveReasoningDuration: (inboxId: string, duration: number) => void;
  // ...
};
```

**StreamingContent Type:**

```typescript
type StreamingContent = {
  content: string; // Main message content
  reasoning: { text: string; status: TextStatus } | null;
  toolCalls: ToolCall[];
  isStreaming: boolean;
  reasoningStartTime: number | null; // For duration calculation
  reasoningDuration: number; // Calculated duration
};
```

### 3. StreamingMessage Component (`components/streaming-message.tsx`)

Wrapper component that subscribes to streaming store for the optimistic assistant message.

**Purpose:**

- Only the optimistic (streaming) message subscribes to streaming store
- Other messages use static React Query data (no re-renders)
- Provides smooth streaming UX without affecting other messages

**Implementation:**

```typescript
export const StreamingMessage = memo(function StreamingMessage({
  message,
  messageId,
  ...
}) {
  const isOptimisticMessage =
    messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
    message.role === 'assistant';

  // Only subscribe for the optimistic message
  const streamingContent = useStreamingContent(
    isOptimisticMessage ? inboxId : '',
  );

  // Merge streaming content with message prop
  const mergedMessage = useMemo(() => {
    if (hasStreamingContent) {
      return {
        ...message,
        content: streamingContent.content || message.content,
        reasoning: streamingContent.reasoning ?? message.reasoning,
        toolCalls: streamingContent.toolCalls.length > 0
          ? streamingContent.toolCalls
          : message.toolCalls,
      };
    }
    return message;
  }, [isOptimisticMessage, message, streamingContent]);

  return <Message message={mergedMessage} ... />;
});
```

### 4. Message List (`components/message-list.tsx`)

Renders the list of messages, using `StreamingMessage` for the optimistic message.

```typescript
const MessageComponent = isOptimisticMessage
  ? StreamingMessage // Subscribes to streaming store
  : Message; // Static, uses React Query data
```

## Data Flow

### Phase 1: Optimistic Update

When user sends a message, `useSendMessageToJob.onMutate` runs **before** the API call:

```typescript
onMutate: async (variables) => {
  // Create optimistic messages immediately
  const newMessages = [
    generateOptimisticUserMessage(variables.message, files),
    generateOptimisticAssistantMessage(variables.provider), // status: 'running'
  ];

  // Update React Query cache
  queryClient.setQueryData(queryKey, (old) => ({
    ...old,
    pages: [...old.pages.slice(0, -1), [...lastPage, ...newMessages]],
  }));
};
```

### Phase 2: WebSocket Streaming

WebSocket message flow:

```
1. ShinkaiMessage (user echo)     → startStreaming(inboxId)
2. Stream (token)                 → Buffer → appendContent/appendReasoning
3. Widget (tool call)             → updateToolCall(inboxId, toolCall, index)
4. Stream (is_done: true)         → flushTokenBuffer({ markComplete: true })
5. ShinkaiMessage (assistant)     → Finalize with real data
```

**Message Type Handling:**

- **User Message**: Initializes streaming state for new message
- **Stream Tokens**: Buffered and flushed at 20 FPS
- **Tool Requests**: Update tool calls in streaming store
- **Final Message**: Contains real message ID, content, reasoning, tool calls

### Phase 3: Finalization

When streaming completes:

```typescript
// 1. Save reasoning duration (persists after clearInbox)
if (streamedContent) {
  const durationToSave = streamedContent.reasoningDuration;
  if (durationToSave > 0) {
    saveReasoningDuration(inboxId, durationToSave);
  }
}

// 2. Update React Query with final data
queryClient.setQueryData(queryKey, (old) => {
  const updated = {
    ...lastMsg,
    messageId: realMessageId, // Replace OPTIMISTIC_ASSISTANT_MESSAGE_ID
    content: finalContent,
    status: { type: 'complete' },
    reasoning: finalReasoning,
    toolCalls: finalToolCalls,
  };
  return { ...old, pages };
});

// 3. Mark streaming as ended (keeps data briefly for StreamingMessage)
endStreaming(inboxId);

// 4. Invalidate if tool calls generated files
if (hasToolCalls) {
  setTimeout(() => {
    queryClient.invalidateQueries({ queryKey });
  }, 1000);
}

// 5. Clear streaming state after delay (2 seconds)
setTimeout(() => {
  clearInbox(inboxId);
}, 2000);
```

## Reasoning Duration Tracking

### Overview

The system tracks reasoning duration client-side for the **last message per inbox**, persisting it even after streaming state is cleared.

### Implementation

1. **During Streaming:**

   - `reasoningStartTime` is set when first reasoning token arrives
   - `reasoningDuration` is calculated when content starts (reasoning ends)

2. **On Finalization:**

   - Duration is saved to persistent `reasoningDurations` map (keyed by `inboxId`)
   - Stored separately from ephemeral streaming state

3. **Display:**

   - Only shown for the last message in inbox (`isLastMessage` check)
   - Retrieved from persistent store via `useReasoningDuration(inboxId)`

4. **Memory Management:**
   - Limited to **last 5 inboxes** maximum
   - Oldest entries are removed when limit is exceeded
   - Uses Map insertion order to track recency

### Code Example

```typescript
// Save duration when finalizing
saveReasoningDuration(inboxId, calculatedDuration);

// Retrieve for display (only last message)
const streamingDuration = useReasoningDuration(isLastMessage ? inboxId : '');
```

## Memory Management

### Cleanup Strategy

1. **Immediate Cleanup (on new message):**

   - Old streaming state cleared when new user message arrives
   - Cancels pending timeouts

2. **Delayed Cleanup (after finalization):**

   - Streaming state cleared after 2 seconds
   - Allows `StreamingMessage` to use final data during transition
   - Prevents brief flash of stale React Query data

3. **Timeout Management:**

   - All `setTimeout` calls are tracked in refs
   - Cancelled on unmount and when new message arrives
   - Prevents memory leaks and race conditions

4. **Reasoning Duration Limits:**
   - Limited to last 5 inboxes
   - Automatically removes oldest entries
   - Prevents unbounded growth

### Cleanup Code Example

```typescript
// Track all timeouts
const flushTimeoutRef = useRef<NodeJS.Timeout | null>(null);
const clearInboxTimeoutRef = useRef<NodeJS.Timeout | null>(null);
const invalidateTimeoutRef = useRef<NodeJS.Timeout | null>(null);

// Cleanup on unmount
useEffect(() => {
  return () => {
    if (flushTimeoutRef.current) clearTimeout(flushTimeoutRef.current);
    if (clearInboxTimeoutRef.current) clearTimeout(clearInboxTimeoutRef.current);
    if (invalidateTimeoutRef.current) clearTimeout(invalidateTimeoutRef.current);
  };
}, []);
```

## Performance Optimizations

### 1. Token Buffering

- **20 FPS update rate** - Smooth enough for human perception
- Batched updates reduce state change frequency
- Prevents excessive re-renders

### 2. Isolated Re-renders

- Only `StreamingMessage` subscribes to streaming store
- Rest of message list doesn't re-render during streaming
- Memoization with custom comparators

### 3. Message Filtering

- Messages filtered by `inboxId` before processing
- Prevents unnecessary updates for other inboxes

### 4. Optimistic Updates

- Immediate UI feedback before API response
- Smooth transition from optimistic to real data

## WebSocket Subscription Management

### Shared Connection

- Single WebSocket connection shared across all hook instances (`{ share: true }`)
- Efficient resource usage

### Topic-Based Subscriptions

- Subscribe to `inbox` topic with `inboxId` as subtopic
- Messages automatically filtered by `inboxId` in handler
- Supports multiple inbox subscriptions simultaneously

### Current Behavior

- Subscriptions persist when switching inboxes
- Messages filtered by current `inboxId`
- Cleanup on component unmount

## File Structure

```
components/chat/
├── context/
│   └── streaming-context.tsx    # Zustand streaming store
├── components/
│   ├── message-list.tsx         # Message list with scroll handling
│   ├── message.tsx              # Individual message component
│   └── streaming-message.tsx    # Wrapper for streaming messages
└── websocket-message.tsx        # WebSocket handler
```

## Usage Example

```typescript
// In chat conversation component
const ChatConversation = () => {
  const { inboxId } = useParams();

  // Enable WebSocket streaming
  useWebSocketMessage({ inboxId, enabled: !!inboxId });
  useWebSocketTools({ inboxId, enabled: !!inboxId });

  // Get messages from React Query
  const { data } = useChatConversationWithOptimisticUpdates({ inboxId });

  return <MessageList messages={data} />;
};
```

## Debugging Tips

### 1. Check streaming store state

```typescript
const content = useStreamingContent(inboxId);
console.log('Streaming:', content?.isStreaming);
console.log('Content length:', content?.content.length);
console.log('Reasoning:', content?.reasoning?.text);
```

### 2. Check WebSocket connection

```typescript
const { readyState } = useWebSocketMessage({ enabled, inboxId });
// readyState: 0=CONNECTING, 1=OPEN, 2=CLOSING, 3=CLOSED
```

### 3. Monitor React Query cache

```typescript
const data = queryClient.getQueryData(queryKey);
console.log('Last message:', data?.pages?.at(-1)?.at(-1));
```

### 4. Check reasoning duration

```typescript
const duration = useReasoningDuration(inboxId);
console.log('Reasoning duration:', duration);
```

## Key Implementation Details

### Race Condition Prevention

- `hasStreamCompletedRef` prevents duplicate finalization
- Timeout cancellation prevents race conditions
- Ordered finalization: React Query update → endStreaming → clearInbox

### State Transition Flow

```
Optimistic Message (status: 'running')
    ↓
Start Streaming (isStreaming: true)
    ↓
Append Tokens (content/reasoning/toolCalls)
    ↓
End Streaming (isStreaming: false, content preserved)
    ↓
Update React Query (status: 'complete', real messageId)
    ↓
Clear Inbox (after 2s delay)
```

### Error Handling

- Try-catch blocks around WebSocket message parsing
- Fallback finalization if parsing fails
- Graceful degradation for missing data

## Future Improvements

Potential enhancements:

- Move to SSE instead of Websockets
