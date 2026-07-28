import { createStore, type StoreApi } from 'zustand';
import type { WorkflowProgressEvent } from '../types/workflow.js';
import type {
  WorkflowHistoryViewState,
  WorkflowRunSummary,
} from '../types/workflow-history.js';
import type {
  WorkflowCollectionState,
  WorkflowMonitorLayout,
  WorkflowNodeConversation,
  WorkflowRunView,
  WorkflowSurface,
} from '../types/workflow-monitor.js';
import {
  pruneTerminalWorkflowRuns,
  reduceWorkflowEvent,
} from './workflow-reducer.js';

const DEFAULT_SPLIT_RATIOS: Record<WorkflowMonitorLayout, number> = {
  'side-by-side': 0.34,
  stacked: 0.4,
};

function collectionState(): WorkflowCollectionState {
  return {
    workflows: new Map(),
    archivedWorkflows: new Map(),
    activeWorkflowId: null,
    selectedNodeIndices: new Map(),
    openWorkflowSurfaces: new Set(),
    pauseRequestedWorkflowIds: new Set(),
    selectionLocked: false,
  };
}

export interface WorkflowStoreState extends WorkflowCollectionState {
  history: WorkflowHistoryViewState;
  monitorLayout: WorkflowMonitorLayout;
  monitorSplitRatios: Record<WorkflowMonitorLayout, number>;
  inputActive: boolean;
  nodeConversations: WorkflowNodeConversation[];

  applyEvent(event: WorkflowProgressEvent): void;
  setActiveWorkflow(workflowId: string): void;
  removeWorkflow(workflowId: string): void;
  openWorkflowHistory(runs: readonly WorkflowRunSummary[]): void;
  closeWorkflowHistory(): void;
  setHistoryRunStatus(
    workflowId: string,
    status: WorkflowRunSummary['status']
  ): void;
  openHistoricalWorkflow(workflow: WorkflowRunView): void;
  setSelectedNode(index: number): void;
  setWorkflowSurfaceOpen(surface: WorkflowSurface, isOpen: boolean): void;
  toggleMonitorLayout(): void;
  setMonitorSplitRatio(layout: WorkflowMonitorLayout, ratio: number): void;
  setPauseRequested(workflowId: string, requested: boolean): void;
  setInputState(active: boolean): void;
  setNodeConversations(conversations: WorkflowNodeConversation[]): void;
  reset(): void;
}

export function createWorkflowStore(
  now: () => number = Date.now
): StoreApi<WorkflowStoreState> {
  return createStore<WorkflowStoreState>((set) => ({
    ...collectionState(),
    history: { isOpen: false, runs: [] },
    monitorLayout: 'side-by-side',
    monitorSplitRatios: { ...DEFAULT_SPLIT_RATIOS },
    inputActive: false,
    nodeConversations: [],

    applyEvent: (event) =>
      set((state) => reduceWorkflowEvent(state, event, now())),
    setActiveWorkflow: (workflowId) =>
      set((state) =>
        state.workflows.has(workflowId) ? { activeWorkflowId: workflowId } : {}
      ),
    removeWorkflow: (workflowId) =>
      set((state) => {
        if (!state.workflows.has(workflowId)) return {};
        const workflows = new Map(state.workflows);
        workflows.delete(workflowId);
        const selectedNodeIndices = new Map(state.selectedNodeIndices);
        selectedNodeIndices.delete(workflowId);
        const activeWorkflowId =
          state.activeWorkflowId === workflowId
            ? (workflows.keys().next().value ?? null)
            : state.activeWorkflowId;
        return { workflows, selectedNodeIndices, activeWorkflowId };
      }),
    openWorkflowHistory: (runs) =>
      set({ history: { isOpen: true, runs: [...runs] } }),
    closeWorkflowHistory: () =>
      set((state) => ({
        history: { ...state.history, isOpen: false },
      })),
    setHistoryRunStatus: (workflowId, status) =>
      set((state) => {
        let changed = false;
        const runs = state.history.runs.map((run) => {
          if (run.workflowId !== workflowId || run.status === status) {
            return run;
          }
          changed = true;
          return { ...run, status };
        });
        return changed ? { history: { ...state.history, runs } } : {};
      }),
    openHistoricalWorkflow: (workflow) =>
      set((state) => ({
        workflows: new Map(state.workflows).set(workflow.workflowId, workflow),
        archivedWorkflows: new Map(state.archivedWorkflows).set(
          workflow.workflowId,
          workflow
        ),
        selectedNodeIndices: new Map(state.selectedNodeIndices).set(
          workflow.workflowId,
          0
        ),
        activeWorkflowId: workflow.workflowId,
      })),
    setSelectedNode: (index) =>
      set((state) => {
        if (
          state.selectionLocked ||
          state.activeWorkflowId === null ||
          !Number.isInteger(index)
        ) {
          return {};
        }
        const workflow = state.workflows.get(state.activeWorkflowId);
        if (!workflow) return {};
        const clamped = Math.min(
          Math.max(0, index),
          Math.max(0, workflow.nodes.length - 1)
        );
        return {
          selectedNodeIndices: new Map(state.selectedNodeIndices).set(
            state.activeWorkflowId,
            clamped
          ),
        };
      }),
    setWorkflowSurfaceOpen: (surface, isOpen) =>
      set((state) => {
        const openWorkflowSurfaces = new Set(state.openWorkflowSurfaces);
        if (isOpen) openWorkflowSurfaces.add(surface);
        else openWorkflowSurfaces.delete(surface);
        const next = { ...state, openWorkflowSurfaces };
        return openWorkflowSurfaces.size === 0
          ? pruneTerminalWorkflowRuns(next)
          : { openWorkflowSurfaces };
      }),
    toggleMonitorLayout: () =>
      set((state) => ({
        monitorLayout:
          state.monitorLayout === 'side-by-side' ? 'stacked' : 'side-by-side',
      })),
    setMonitorSplitRatio: (layout, ratio) =>
      set((state) => ({
        monitorSplitRatios: {
          ...state.monitorSplitRatios,
          [layout]: Math.max(0.2, Math.min(0.8, ratio)),
        },
      })),
    setPauseRequested: (workflowId, requested) =>
      set((state) => {
        const pauseRequestedWorkflowIds = new Set(
          state.pauseRequestedWorkflowIds
        );
        if (requested) pauseRequestedWorkflowIds.add(workflowId);
        else pauseRequestedWorkflowIds.delete(workflowId);
        return { pauseRequestedWorkflowIds };
      }),
    setInputState: (active) =>
      set({ inputActive: active, selectionLocked: active }),
    setNodeConversations: (nodeConversations) => set({ nodeConversations }),
    reset: () =>
      set({
        ...collectionState(),
        history: { isOpen: false, runs: [] },
        monitorLayout: 'side-by-side',
        monitorSplitRatios: { ...DEFAULT_SPLIT_RATIOS },
        inputActive: false,
        nodeConversations: [],
      }),
  }));
}

export const workflowStore = createWorkflowStore();

export function selectActiveWorkflow(
  state: WorkflowStoreState
): WorkflowRunView | null {
  return state.activeWorkflowId
    ? (state.workflows.get(state.activeWorkflowId) ?? null)
    : null;
}

export function selectWorkflowNodeIndex(state: WorkflowStoreState): number {
  return state.activeWorkflowId
    ? (state.selectedNodeIndices.get(state.activeWorkflowId) ?? 0)
    : 0;
}

export function selectLiveWorkflowCount(state: WorkflowStoreState): number {
  let count = 0;
  for (const workflow of state.workflows.values()) {
    if (workflow.status === 'running' || workflow.status === 'paused') {
      count += 1;
    }
  }
  return count;
}
