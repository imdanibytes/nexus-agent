import { MessageSquare } from "lucide-react";

export function EmptyState() {
  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="text-center">
        <MessageSquare size={48} className="mx-auto mb-4 text-nx-muted" />
        <h2 className="text-lg font-medium text-nx-text mb-1">Nexus Agent</h2>
        <p className="text-sm text-nx-muted">
          Start a new conversation or select one from the sidebar.
        </p>
      </div>
    </div>
  );
}
