export type TaskStatus = "pending" | "in_progress" | "completed" | "failed";

export type AgentMode = "general" | "discovery" | "planning" | "execution" | "validation";

export interface Task {
  id: string;
  title: string;
  description?: string;
  status: TaskStatus;
  parentId?: string;
  dependsOn: string[];
  activeLabel?: string;
  metadata?: Record<string, unknown>;
  createdAt: number;
  updatedAt: number;
  completedAt?: number;
}

export interface Plan {
  id: string;
  conversationId: string;
  title: string;
  summary?: string;
  taskIds: string[];
  approved: boolean | null;
  filePath?: string;
  createdAt: number;
  updatedAt: number;
}

export interface TaskState {
  plan: Plan | null;
  tasks: Record<string, Task>;
  mode: AgentMode;
}
