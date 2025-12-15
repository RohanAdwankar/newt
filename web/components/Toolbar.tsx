import React from 'react';
import { Save, Plus, Scissors, Copy, Clipboard, Play, Square, RotateCw, Layers, Settings, Folder, Moon, Sun, Keyboard, Timer, Share, Maximize2, Minimize2 } from 'lucide-react';

interface ToolbarProps {
  onSave: () => void;
  onShare: () => void;
  onNewCell: () => void;
  onCut: () => void;
  onCopy: () => void;
  onPaste: () => void;
  onRun: () => void;
  onStop: () => void;
  onRestart: () => void;
  onRunAll: () => void;
  onPolling: () => void;
  executionMode?: 'client' | 'remote';
  onExecutionModeChange?: (mode: 'client' | 'remote') => void;
  cellType?: string;
  onCellTypeChange?: (type: string) => void;
  onToggleSidebar: () => void;
  onToggleTheme: () => void;
  theme: string;
  vimMode: boolean;
  onToggleVimMode: () => void;
  isFullscreen: boolean;
  onToggleFullscreen: () => void;
}

export const Toolbar: React.FC<ToolbarProps> = ({
  onSave,
  onShare,
  onNewCell,
  onCut,
  onCopy,
  onPaste,
  onRun,
  onStop,
  onRestart,
  onRunAll,
  onPolling,
  executionMode = 'remote',
  onExecutionModeChange,
  cellType,
  onCellTypeChange,
  onToggleSidebar,
  onToggleTheme,
  theme,
  vimMode,
  onToggleVimMode,
  isFullscreen,
  onToggleFullscreen
}) => {
  const btnClass = "p-1.5 rounded hover:bg-bg-tertiary text-text-secondary hover:text-text-primary transition-colors";

  return (
    <div className="h-10 border-b border-border-color bg-bg-secondary flex items-center px-2 gap-1 shrink-0">
      <button onClick={onToggleSidebar} className={btnClass} title="Toggle File Browser (Space+E)">
        <Folder size={18} />
      </button>
      <div className="w-px h-6 bg-border-color mx-1" />
      <button onClick={onSave} className={btnClass} title="Save (Ctrl+S)">
        <Save size={18} />
      </button>
      <button onClick={onShare} className={btnClass} title="Export to Markdown">
        <Share size={18} />
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
      <button onClick={onPolling} className={btnClass} title="Set Polling Interval">
        <Timer size={18} />
      </button>
      <button onClick={onToggleFullscreen} className={btnClass} title={isFullscreen ? "Exit Fullscreen (Esc)" : "Fullscreen (F)"}>
        {isFullscreen ? <Minimize2 size={18} /> : <Maximize2 size={18} />}
      </button>

      <div className="flex-1" />

      <button onClick={onToggleVimMode} className={btnClass} title={vimMode ? "Disable Vim Mode" : "Enable Vim Mode"}>
        <Keyboard size={18} className={vimMode ? "text-accent" : ""} />
      </button>
      <button onClick={onToggleTheme} className={btnClass} title="Toggle Theme">
        {theme === 'dark' ? <Moon size={18} /> : <Sun size={18} />}
      </button>

      {onCellTypeChange && cellType && (
        <div className="flex items-center gap-2 mr-2 ml-2">
          <span className="text-xs text-text-secondary">Type:</span>
          <select
            value={cellType}
            onChange={(e) => onCellTypeChange(e.target.value)}
            className="bg-bg-tertiary text-text-primary text-xs rounded px-2 py-1 border border-border-color outline-none"
          >
            <option value="python">Python</option>
            <option value="rust">Rust</option>
            <option value="javascript">JavaScript</option>
            <option value="typescript">TypeScript</option>
            <option value="c">C</option>
            <option value="cpp">C++</option>
            <option value="go">Go</option>
            <option value="shell">Shell</option>
          </select>
        </div>
      )}

      {onExecutionModeChange && (
        <div className="flex items-center gap-2 mr-2 ml-2">
          <span className="text-xs text-text-secondary">Mode:</span>
          <select
            value={executionMode}
            onChange={(e) => onExecutionModeChange(e.target.value as any)}
            className="bg-bg-tertiary text-text-primary text-xs rounded px-2 py-1 border border-border-color outline-none"
          >
            <option value="remote">Server</option>
            <option value="client">Browser</option>
          </select>
        </div>
      )}
    </div>
  );
};
