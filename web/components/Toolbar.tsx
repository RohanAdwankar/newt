import React from 'react';
import { Save, Plus, Scissors, Copy, Clipboard, Play, Square, RotateCw, Layers, Settings } from 'lucide-react';

interface ToolbarProps {
  onSave: () => void;
  onNewCell: () => void;
  onCut: () => void;
  onCopy: () => void;
  onPaste: () => void;
  onRun: () => void;
  onStop: () => void;
  onRestart: () => void;
  onRunAll: () => void;
  executionMode?: 'client' | 'remote';
  onExecutionModeChange?: (mode: 'client' | 'remote') => void;
}

export const Toolbar: React.FC<ToolbarProps> = ({
  onSave,
  onNewCell,
  onCut,
  onCopy,
  onPaste,
  onRun,
  onStop,
  onRestart,
  onRunAll,
  executionMode = 'remote',
  onExecutionModeChange
}) => {
  const btnClass = "p-1.5 rounded hover:bg-bg-tertiary text-text-secondary hover:text-text-primary transition-colors";

  return (
    <div className="h-10 border-b border-border-color bg-bg-secondary flex items-center px-2 gap-1 shrink-0">
      <button onClick={onSave} className={btnClass} title="Save (Ctrl+S)">
        <Save size={18} />
      </button>
      <div className="w-px h-6 bg-border-color mx-1" />
      <button onClick={onNewCell} className={btnClass} title="New Cell (A/B)">
        <Plus size={18} />
      </button>
      <button onClick={onCut} className={btnClass} title="Cut Cell (X)">
        <Scissors size={18} />
      </button>
      <button onClick={onCopy} className={btnClass} title="Copy Cell (C)">
        <Copy size={18} />
      </button>
      <button onClick={onPaste} className={btnClass} title="Paste Cell (V)">
        <Clipboard size={18} />
      </button>
      <div className="w-px h-6 bg-border-color mx-1" />
      <button onClick={onRun} className={btnClass} title="Run Cell (Ctrl+Enter)">
        <Play size={18} />
      </button>
      <button onClick={onStop} className={btnClass} title="Stop">
        <Square size={18} />
      </button>
      <button onClick={onRestart} className={btnClass} title="Restart Kernel">
        <RotateCw size={18} />
      </button>
      <button onClick={onRunAll} className={btnClass} title="Run All">
        <Layers size={18} />
      </button>
      
      <div className="flex-1" />
      
      {onExecutionModeChange && (
        <div className="flex items-center gap-2 mr-2">
          <span className="text-xs text-text-secondary">Mode:</span>
          <select 
            value={executionMode} 
            onChange={(e) => onExecutionModeChange(e.target.value as any)}
            className="bg-bg-tertiary text-text-primary text-xs rounded px-2 py-1 border border-border-color outline-none"
          >
            <option value="remote">Remote (Core API)</option>
            <option value="client">Client (Browser)</option>
          </select>
        </div>
      )}
    </div>
  );
};
