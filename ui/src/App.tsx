export default function App() {
  return (
    <div className="relative flex h-full flex-col overflow-hidden rounded-2xl bg-default-100/60 dark:bg-default-50/40 backdrop-blur-xl border border-default-200 dark:border-default-200/50">
      <div className="flex items-center h-9 shrink-0 border-b border-default-200/50 px-3">
        <span className="text-xs font-semibold text-default-500">Nexus</span>
      </div>
      <div className="flex-1 flex items-center justify-center min-h-0">
        <p className="text-sm text-default-400">Ready.</p>
      </div>
    </div>
  );
}
