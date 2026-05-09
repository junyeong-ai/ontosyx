import type { ChatMessage, ToolCall } from "@/lib/store";
import type { SessionMessage } from "@/types/api";

type SessionToolCall = NonNullable<SessionMessage["tool_calls"]>[number];

function restoreToolStatus(status: SessionToolCall["status"]): ToolCall["status"] {
  return status;
}

export function restoreChatMessages(messages: SessionMessage[]): ChatMessage[] {
  return messages.map((message, index) => ({
    id: `restored-${index}`,
    role: message.role,
    content: message.content,
    thinking: message.thinking ?? undefined,
    toolCalls: message.tool_calls?.map((toolCall) => ({
      id: toolCall.id,
      name: toolCall.name,
      input: toolCall.input ?? undefined,
      output: toolCall.output ?? undefined,
      status: restoreToolStatus(toolCall.status),
      durationMs: toolCall.duration_ms ?? undefined,
    })),
  }));
}
