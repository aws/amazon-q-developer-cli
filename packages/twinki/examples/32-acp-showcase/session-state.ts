import { sanitizeTerminalText } from "twinki";
import type * as acp from "@agentclientprotocol/sdk";
import type { AcpEvent, SessionState, ToolBlock, ToolState, TranscriptBlock } from "./types.js";

export type SessionAction = AcpEvent | { type: "user_message"; text: string };

export const initialSessionState: SessionState = {
  connection: "connecting",
  agentName: "ACP",
  blocks: [],
  permission: null,
  contextPercent: 0,
  mode: "",
  nextBlockId: 1,
};

function clean(value: string): string {
  return sanitizeTerminalText(value);
}

function encode(value: unknown): string | undefined {
  if (typeof value === "string") return clean(value);
  if (value === undefined) return undefined;
  try {
    return clean(JSON.stringify(value));
  } catch {
    return undefined;
  }
}

function textContent(value: unknown): string | undefined {
  if (typeof value !== "object" || value === null) return undefined;
  const content = value as { type?: unknown; text?: unknown };
  return content.type === "text" && typeof content.text === "string" ? clean(content.text) : undefined;
}

function toolState(value: acp.ToolCallStatus | null | undefined): ToolState {
  if (value === "completed") return "done";
  if (value === "failed") return "error";
  return "running";
}

function toolOutput(update: acp.ToolCallUpdate): string | undefined {
  const content = update.content
    ?.flatMap((item): string[] => {
      if (item.type === "content") {
        const text = textContent(item.content);
        return text ? [text] : [];
      }
      if (item.type === "diff") return [`Updated ${item.path}`];
      return [];
    })
    .join("\n");
  return content || encode(update.rawOutput) || encode(update.rawInput);
}

function cap(blocks: TranscriptBlock[]): TranscriptBlock[] {
  return blocks.length > 300 ? blocks.slice(-300) : blocks;
}

function appendText(state: SessionState, kind: "agent" | "thought", text: string): SessionState {
  const value = clean(text);
  const tail = state.blocks.at(-1);
  if (tail?.kind === kind) {
    return {
      ...state,
      blocks: [...state.blocks.slice(0, -1), { ...tail, text: tail.text + value }],
    };
  }
  return {
    ...state,
    nextBlockId: state.nextBlockId + 1,
    blocks: cap([...state.blocks, { id: state.nextBlockId, kind, text: value }]),
  };
}

function addNotice(state: SessionState, message: string): SessionState {
  return {
    ...state,
    nextBlockId: state.nextBlockId + 1,
    blocks: cap([...state.blocks, { id: state.nextBlockId, kind: "notice", text: clean(message) }]),
  };
}

function applyUpdate(state: SessionState, update: acp.SessionUpdate): SessionState {
  switch (update.sessionUpdate) {
    case "agent_message_chunk": {
      const text = textContent(update.content);
      return text ? appendText(state, "agent", text) : state;
    }
    case "agent_thought_chunk": {
      const text = textContent(update.content);
      return text ? appendText(state, "thought", text) : state;
    }
    case "tool_call":
      return {
        ...state,
        nextBlockId: state.nextBlockId + 1,
        blocks: cap([
          ...state.blocks,
          {
            id: state.nextBlockId,
            kind: "tool",
            toolId: update.toolCallId,
            title: clean(update.title),
            state: toolState(update.status),
            detail: encode(update.rawInput),
          },
        ]),
      };
    case "tool_call_update": {
      const detail = toolOutput(update);
      return {
        ...state,
        blocks: state.blocks.map((block) =>
          block.kind === "tool" && block.toolId === update.toolCallId
            ? ({
                ...block,
                title: update.title ? clean(update.title) : block.title,
                state: update.status ? toolState(update.status) : block.state,
                detail: detail ?? block.detail,
              } satisfies ToolBlock)
            : block,
        ),
      };
    }
    case "usage_update":
      return {
        ...state,
        contextPercent: update.size > 0 ? Math.min(100, (update.used / update.size) * 100) : 0,
      };
    case "current_mode_update":
      return { ...state, mode: update.currentModeId };
    default:
      return state;
  }
}

export function sessionReducer(state: SessionState, action: SessionAction): SessionState {
  switch (action.type) {
    case "connection":
      return {
        ...state,
        connection: action.state,
        agentName: action.agentName ?? state.agentName,
      };
    case "permission":
      return { ...state, permission: action.prompt };
    case "session_update":
      return applyUpdate(state, action.update);
    case "user_message":
      return {
        ...state,
        connection: "running",
        nextBlockId: state.nextBlockId + 1,
        blocks: cap([...state.blocks, { id: state.nextBlockId, kind: "user", text: clean(action.text) }]),
      };
    case "turn_done":
      return {
        ...state,
        connection: state.connection === "error" ? "error" : "ready",
      };
    case "error":
      return addNotice({ ...state, connection: "error" }, action.message);
  }
}
