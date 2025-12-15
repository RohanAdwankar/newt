import React from 'react';
import { Folder, File, Plus, Sun, Moon } from 'lucide-react';
import { clsx } from 'clsx';

interface SidebarProps {
  files: string[];
  selectedIndex: number;
  focused: boolean;
  visible: boolean;
  theme: string;
  onToggleTheme: () => void;
  onFileClick: (index: number) => void;
  onFileContextMenu: (e: React.MouseEvent, index: number) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({ 
  files, 
  selectedIndex, 
  focused, 
  visible, 
  theme, 
  onToggleTheme,
  onFileClick,
  onFileContextMenu
}) => {
  if (!visible) return null;

  return (
    <div className={clsx(
      "w-64 h-full border-r border-border-color flex flex-col bg-bg-secondary transition-colors shrink-0",
      focused ? "border-r-2 border-r-accent" : "border-r border-border-color"
    )}>
      <div className="p-2 border-b border-border-color font-bold text-accent flex items-center gap-2">
        <Folder size={16} />
        <span>Files</span>
      </div>
      <div className="flex-1 overflow-y-auto">
        <div 
          className={clsx(
            "p-2 cursor-pointer flex items-center gap-2 text-sm font-mono hover:bg-bg-tertiary",
            selectedIndex === 0 && focused ? "bg-selection text-accent" : "text-text-secondary"
          )}
          onClick={() => onFileClick(0)}
          onContextMenu={(e) => {
            e.preventDefault();
            onFileContextMenu(e, 0);
          }}
        >
          <Plus size={16} />
          <span>New Notebook</span>
        </div>
        {files.map((file, index) => (
          <div
            key={file}
            className={clsx(
              "p-2 cursor-pointer flex items-center gap-2 truncate text-sm font-mono hover:bg-bg-tertiary",
              selectedIndex === index + 1 && focused ? "bg-selection text-accent" : "text-text-secondary"
            )}
            onClick={() => onFileClick(index + 1)}
            onContextMenu={(e) => {
                e.preventDefault();
                onFileContextMenu(e, index + 1);
            }}
          >
            <File size={16} />
            <span>{file}</span>
          </div>
        ))}
      </div>
      <div className="p-2 border-t border-border-color">
        <button 
          onClick={onToggleTheme}
          className="w-full flex items-center justify-center gap-2 p-2 rounded bg-bg-tertiary text-text-primary hover:bg-selection transition-colors"
        >
          {theme === 'dark' ? <Sun size={16} /> : <Moon size={16} />}
          <span className="text-sm font-mono">{theme === 'dark' ? 'Light Mode' : 'Dark Mode'}</span>
        </button>
      </div>
    </div>
  );
};
