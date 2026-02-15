import { useState } from "react";
import { Save, Trash2, ArrowLeft } from "lucide-react";
import type { AgentProfile, ModelInfo } from "@/api/client.js";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";

interface Props {
  profile?: AgentProfile;
  models: ModelInfo[];
  onSave: (data: { name: string; model: string; systemPrompt: string; avatar?: string }) => void;
  onDelete?: () => void;
  onCancel: () => void;
}

export function ProfileEditor({ profile, models, onSave, onDelete, onCancel }: Props) {
  const [name, setName] = useState(profile?.name || "");
  const [avatar, setAvatar] = useState(profile?.avatar || "");
  const [model, setModel] = useState(profile?.model || "");
  const [systemPrompt, setSystemPrompt] = useState(profile?.systemPrompt || "");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !model.trim()) return;
    onSave({
      name: name.trim(),
      model: model.trim(),
      systemPrompt: systemPrompt.trim(),
      avatar: avatar.trim() || undefined,
    });
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-5">
      {/* Header */}
      <div className="flex items-center gap-2">
        <Button type="button" variant="ghost" size="icon" onClick={onCancel} className="h-7 w-7">
          <ArrowLeft size={14} />
        </Button>
        <div>
          <h3 className="text-sm font-medium">
            {profile ? "Edit Profile" : "New Profile"}
          </h3>
          <p className="text-[11px] text-muted-foreground">
            {profile ? "Update this profile's settings." : "Create a named model + prompt combination."}
          </p>
        </div>
      </div>

      <Separator />

      {/* Avatar + Name */}
      <div className="flex gap-3">
        <div className="flex-shrink-0 space-y-1.5">
          <Label htmlFor="avatar" className="text-xs">Avatar</Label>
          <Input
            id="avatar"
            value={avatar}
            onChange={(e) => setAvatar(e.target.value)}
            placeholder="🤖"
            maxLength={2}
            className="w-14 text-center"
          />
        </div>
        <div className="flex-1 space-y-1.5">
          <Label htmlFor="name" className="text-xs">Name</Label>
          <Input
            id="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Research Assistant"
            required
          />
        </div>
      </div>

      {/* Model */}
      <div className="space-y-1.5">
        <Label htmlFor="model" className="text-xs">Model</Label>
        <Input
          id="model"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="qwen3:30b"
          list={models.length > 0 ? "profile-model-options" : undefined}
          required
          className="font-mono text-xs"
        />
        {models.length > 0 && (
          <datalist id="profile-model-options">
            {models.map((m) => (
              <option key={m.id} value={m.id} />
            ))}
          </datalist>
        )}
        <p className="text-[11px] text-muted-foreground">
          {models.length > 0
            ? "Type to search or select from discovered models."
            : "Enter a model ID. Test the connection to see available models."}
        </p>
      </div>

      {/* System Prompt */}
      <div className="space-y-1.5">
        <Label htmlFor="prompt" className="text-xs">System Prompt</Label>
        <Textarea
          id="prompt"
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
          placeholder="You are a helpful assistant..."
          rows={5}
          className="resize-none text-xs"
        />
      </div>

      <Separator />

      {/* Actions */}
      <div className="flex items-center gap-2">
        <Button type="submit" size="sm" disabled={!name.trim() || !model.trim()} className="gap-1.5">
          <Save size={13} />
          Save
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          Cancel
        </Button>
        {profile && onDelete && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onDelete}
            className="ml-auto text-destructive hover:text-destructive gap-1.5"
          >
            <Trash2 size={13} />
            Delete
          </Button>
        )}
      </div>
    </form>
  );
}
