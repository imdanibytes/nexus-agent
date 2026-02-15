export function StreamingCursor() {
  return (
    <div className="flex items-center gap-1 py-2">
      <span
        className="w-1.5 h-1.5 rounded-full bg-primary animate-dot-pulse"
        style={{ animationDelay: "0ms" }}
      />
      <span
        className="w-1.5 h-1.5 rounded-full bg-primary animate-dot-pulse"
        style={{ animationDelay: "200ms" }}
      />
      <span
        className="w-1.5 h-1.5 rounded-full bg-primary animate-dot-pulse"
        style={{ animationDelay: "400ms" }}
      />
    </div>
  );
}
