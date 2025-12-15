import React from 'react';
import { clsx } from 'clsx';

interface CommandBarProps {
  mode: 'normal' | 'editing' | 'command' | 'renaming' | 'polling';
  commandInput: string;
  pollingInput?: string;
  statusMessage: string | null;
  filePath: string | null;
}

export const CommandBar: React.FC<CommandBarProps> = ({ mode, commandInput, pollingInput, statusMessage, filePath }) => {
  return (
    <div className="h-8 border-t border-border-color bg-bg-secondary flex items-center px-2 text-sm font-mono shrink-0">
      {mode === 'command' ? (
        <div className="flex items-center w-full text-text-primary">
          <span className="text-accent mr-1">:</span>
          <span className="flex-1 outline-none bg-transparent">{commandInput}</span>
        </div>
      ) : mode === 'polling' ? (
        <div className="flex items-center w-full text-text-primary">
          <span className="text-accent mr-1">Polling:</span>
          <span className="flex-1 outline-none bg-transparent">{pollingInput}</span>
        </div>
      ) : (
        <div className="flex justify-between w-full text-text-muted">
          <span className="truncate">{statusMessage || filePath || "[No Name]"}</span>
          <span className="uppercase text-xs bg-bg-tertiary px-1 rounded text-accent ml-2">{mode}</span>
        </div>
      )}
    </div>
  );
};
