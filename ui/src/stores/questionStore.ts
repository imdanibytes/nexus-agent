import { create } from "zustand";

export interface PendingQuestion {
  questionId: string;
  toolCallId: string;
  question: string;
  type: "confirm" | "select" | "multi_select" | "text";
  options?: Array<{ value: string; label: string; description?: string }>;
  context?: string;
  placeholder?: string;
}

interface QuestionStoreState {
  /** Pending questions keyed by toolCallId */
  questions: Record<string, PendingQuestion>;
  setPending: (q: PendingQuestion) => void;
  setAnswered: (toolCallId: string) => void;
  remove: (toolCallId: string) => void;
}

export const useQuestionStore = create<QuestionStoreState>((set) => ({
  questions: {},

  setPending: (q) =>
    set((s) => ({
      questions: { ...s.questions, [q.toolCallId]: q },
    })),

  setAnswered: (toolCallId) =>
    set((s) => {
      const { [toolCallId]: _, ...rest } = s.questions;
      return { questions: rest };
    }),

  remove: (toolCallId) =>
    set((s) => {
      const { [toolCallId]: _, ...rest } = s.questions;
      return { questions: rest };
    }),
}));
