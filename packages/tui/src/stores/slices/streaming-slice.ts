/**
 * Streaming slice - manages streaming buffer state with proper typing
 */
import type { StateCreator } from 'zustand';

export interface StreamingBufferControl {
  startBuffering: (() => void) | null;
  stopBuffering: (() => void) | null;
}

export interface StreamingState {
  streamingBuffer: StreamingBufferControl;
}

export interface StreamingActions {
  setStreamingBuffer: (control: StreamingBufferControl) => void;
  clearStreamingBuffer: () => void;
}

export type StreamingSlice = StreamingState & StreamingActions;

export const createStreamingSlice: StateCreator<StreamingSlice> = (set) => ({
  // State
  streamingBuffer: { startBuffering: null, stopBuffering: null },

  // Actions
  setStreamingBuffer: (control) => set({ streamingBuffer: control }),
  clearStreamingBuffer: () => set({ streamingBuffer: { startBuffering: null, stopBuffering: null } }),
});
