export interface TaskItem {
  id: string;
  subject: string;
  status: 'pending' | 'completed';
}

/** Shape returned by the task tool backend (format_full_state). */
export interface RawTask {
  id: string;
  task_description?: string;
  subject?: string;
  completed: boolean;
}
