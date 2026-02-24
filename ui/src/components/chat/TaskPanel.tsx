import { type FC } from "react";
import {
  CheckCircle2Icon,
  CircleDotIcon,
  CircleIcon,
  XCircleIcon,
  ChevronRightIcon,
  ChevronLeftIcon,
  ListTodoIcon,
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { Button, Chip, CircularProgress, Progress } from "@heroui/react";
import { cn } from "../../lib/utils";
import { useTaskStore } from "../../stores/taskStore";
import { useThreadListStore } from "../../stores/threadListStore";
import { ModeChip } from "./ModeChip";
import type { Task, TaskStatus } from "../../types/tasks";

// ── Status icon ──

function StatusIcon({ status }: { status: TaskStatus }) {
  switch (status) {
    case "completed":
      return <CheckCircle2Icon size={13} className="text-success shrink-0 mt-px" />;
    case "in_progress":
      return <CircleDotIcon size={13} className="text-primary animate-pulse shrink-0 mt-px" />;
    case "failed":
      return <XCircleIcon size={13} className="text-danger shrink-0 mt-px" />;
    default:
      return <CircleIcon size={13} className="text-default-300 shrink-0 mt-px" />;
  }
}

// ── Task row ──

const TaskRow: FC<{ task: Task; isSubtask?: boolean; isCurrent?: boolean }> = ({
  task,
  isSubtask,
  isCurrent,
}) => (
  <div
    className={cn(
      "flex items-start gap-2 py-1.5 px-2 rounded-lg text-xs transition-colors",
      isSubtask && "ml-5",
      isCurrent && "bg-primary/5 dark:bg-primary/10",
      task.status === "completed" && "opacity-50",
    )}
  >
    <StatusIcon status={task.status} />
    <span
      className={cn(
        "leading-tight",
        task.status === "completed" && "line-through text-default-400",
        task.status === "in_progress" && "text-default-900 font-medium",
        task.status === "pending" && "text-default-600",
      )}
    >
      {task.status === "in_progress" && task.activeLabel
        ? task.activeLabel
        : task.title}
    </span>
  </div>
);

// ── Approval chip (display-only) ──

const ApprovalChip: FC<{ approved: boolean | null }> = ({ approved }) => {
  if (approved === null) {
    return (
      <Chip size="sm" variant="flat" color="warning" className="text-[10px] h-5">
        Awaiting approval
      </Chip>
    );
  }
  if (approved) {
    return (
      <Chip size="sm" variant="flat" color="success" className="text-[10px] h-5">
        Approved
      </Chip>
    );
  }
  return (
    <Chip size="sm" variant="flat" color="danger" className="text-[10px] h-5">
      Rejected
    </Chip>
  );
};

// ── Spring config ──

const SPRING = { type: "spring" as const, stiffness: 400, damping: 35 };
const FADE = { duration: 0.15 };

// ── Main panel ──

export const TaskPanel: FC = () => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const taskState = useTaskStore((s) =>
    activeThreadId ? s.states[activeThreadId] : undefined,
  );
  const panelOpen = useTaskStore((s) => s.panelOpen);
  const setPanelOpen = useTaskStore((s) => s.setPanelOpen);

  // Don't render if no plan and in general mode
  if (!taskState?.plan && (!taskState || taskState.mode === "general")) return null;

  const mode = taskState?.mode ?? "general";
  const plan = taskState?.plan;
  const tasks = taskState?.tasks ?? {};

  const orderedTasks = plan
    ? plan.taskIds.map((id) => tasks[id]).filter(Boolean)
    : [];

  const topLevelTasks = orderedTasks.filter((t) => !t.parentId);
  const subtasksOf = (parentId: string) =>
    orderedTasks.filter((t) => t.parentId === parentId);

  const completed = orderedTasks.filter((t) => t.status === "completed").length;
  const total = orderedTasks.length;
  const pct = total > 0 ? Math.round((completed / total) * 100) : 0;

  // Find current task
  const currentTask =
    orderedTasks.find((t) => t.status === "in_progress") ||
    orderedTasks.find(
      (t) =>
        t.status === "pending" &&
        t.dependsOn.every((dep) => tasks[dep]?.status === "completed"),
    );

  return (
    <motion.div
      className="shrink-0 h-full"
      initial={false}
      animate={{ width: panelOpen ? 256 : 44 }}
      transition={SPRING}
    >
      <div className="h-full rounded-xl bg-white/60 dark:bg-default-50/30 backdrop-blur-xl border border-default-200 dark:border-default-200/50 overflow-hidden">
        <AnimatePresence mode="wait" initial={false}>
          {panelOpen ? (
            <motion.div
              key="expanded"
              className="flex flex-col h-full"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={FADE}
            >
              {/* Header */}
              <div className="flex items-center justify-between gap-2 px-3 py-2.5 border-b border-default-200/50">
                <div className="flex items-center gap-2 min-w-0 flex-1">
                  <ListTodoIcon size={14} className="text-default-400 shrink-0" />
                  <span className="text-xs font-semibold truncate text-default-900">
                    {plan?.title ?? "Workflow"}
                  </span>
                </div>
                <div className="flex items-center gap-1.5 shrink-0">
                  <ModeChip mode={mode} />
                  <Button
                    variant="light"
                    size="sm"
                    onPress={() => setPanelOpen(false)}
                    isIconOnly
                    className="h-6 w-6 min-w-6 p-0 text-default-400"
                  >
                    <ChevronRightIcon size={12} />
                  </Button>
                </div>
              </div>

              {/* Summary + approval + progress */}
              <div className="px-3 py-2.5 space-y-2.5 border-b border-default-200/50">
                {plan?.summary && (
                  <p className="text-[11px] text-default-500 leading-relaxed">
                    {plan.summary}
                  </p>
                )}
                <div className="flex items-center gap-2">
                  {plan && (
                    <ApprovalChip approved={plan.approved} />
                  )}
                </div>
                {total > 0 && (
                  <div className="space-y-1">
                    <Progress
                      size="sm"
                      value={pct}
                      color="success"
                      className="h-1.5"
                      aria-label="Task progress"
                    />
                    <div className="flex justify-between text-[10px] text-default-400">
                      <span>
                        {completed} of {total} tasks
                      </span>
                      <span className="font-medium tabular-nums">{pct}%</span>
                    </div>
                  </div>
                )}
              </div>

              {/* Task list */}
              <div className="flex-1 overflow-y-auto py-1.5 px-1.5">
                {topLevelTasks.map((task) => (
                  <div key={task.id}>
                    <TaskRow task={task} isCurrent={task === currentTask} />
                    {subtasksOf(task.id).map((sub) => (
                      <TaskRow
                        key={sub.id}
                        task={sub}
                        isSubtask
                        isCurrent={sub === currentTask}
                      />
                    ))}
                  </div>
                ))}
                {plan && orderedTasks.length === 0 && (
                  <div className="text-[11px] text-default-500 text-center py-6">
                    No tasks yet
                  </div>
                )}
                {!plan && mode !== "general" && (
                  <div className="text-[11px] text-default-500 text-center py-6">
                    {mode === "discovery"
                      ? "Gathering requirements..."
                      : "No plan created yet"}
                  </div>
                )}
              </div>
            </motion.div>
          ) : (
            <motion.div
              key="collapsed"
              className="flex flex-col items-center gap-1.5 py-2 px-1 h-full"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={FADE}
            >
              <Button
                variant="flat"
                size="sm"
                onPress={() => setPanelOpen(true)}
                isIconOnly
                className="h-7 w-7 min-w-7 rounded-lg"
                aria-label="Show tasks"
              >
                <ChevronLeftIcon size={14} />
              </Button>

              {total > 0 && (
                <CircularProgress
                  size="sm"
                  value={pct}
                  color="success"
                  aria-label="Task progress"
                  classNames={{ svg: "w-7 h-7" }}
                />
              )}

              {/* Task status icons */}
              <div className="flex flex-col items-center gap-1 flex-1 overflow-y-auto py-1">
                {orderedTasks.map((task) => (
                  <StatusIcon key={task.id} status={task.status} />
                ))}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </motion.div>
  );
};
