import type { FC } from "react";
import { MessageSquareIcon, SearchIcon, SparklesIcon } from "lucide-react";

const SUGGESTIONS = [
  { icon: MessageSquareIcon, label: "Ask me anything", prompt: "" },
  { icon: SparklesIcon, label: "Build something", prompt: "Help me build " },
  { icon: SearchIcon, label: "Research a topic", prompt: "Research " },
];

export const ThreadWelcome: FC<{ onSend: (text: string) => void }> = ({
  onSend,
}) => {
  return (
    <div className="mx-auto my-auto flex w-full max-w-(--thread-max-width) grow flex-col">
      <div className="flex w-full grow flex-col items-center justify-center gap-6">
        <div className="flex flex-col items-center gap-2 animate-fade-in">
          <div className="flex size-12 items-center justify-center rounded-xl bg-default-100 dark:bg-default-50/40 backdrop-blur-xl border border-default-200 dark:border-default-200/50 shadow-sm dark:shadow-none">
            <SparklesIcon className="size-6 text-primary" />
          </div>
          <h1 className="text-xl font-semibold text-default-900">Nexus</h1>
          <p
            className="text-sm text-default-500 animate-fade-in"
            style={{ animationDelay: "75ms" }}
          >
            What would you like to work on?
          </p>
        </div>

        <div
          className="flex flex-wrap justify-center gap-3 animate-fade-in"
          style={{ animationDelay: "150ms" }}
        >
          {SUGGESTIONS.map((s) => {
            const Icon = s.icon;
            return (
              <button
                key={s.label}
                type="button"
                onClick={() => s.prompt && onSend(s.prompt)}
                className="group flex items-center gap-2 rounded-xl border border-default-200 dark:border-default-200/50 bg-white dark:bg-default-50/40 backdrop-blur-xl shadow-sm dark:shadow-none px-4 py-2.5 text-sm text-default-700 transition-all hover:bg-default-100 dark:hover:bg-default-100/50 hover:text-default-900 hover:border-default-300 dark:hover:border-default-300/50"
              >
                <Icon className="size-4 text-default-400 group-hover:text-primary transition-colors" />
                {s.label}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
};
