import React from 'react';
import { Folder, File, Plus, ChevronRight, ChevronDown } from 'lucide-react';
import { clsx } from 'clsx';

export interface FileItem {
  path: string | null;
  label: string;
  is_header: boolean;
  is_app_file: boolean;
  is_directory: boolean;
  is_expanded: boolean;
  depth: number;
}

interface SidebarProps {
  files: FileItem[];
  isBackendAvailable: boolean;
  selectedIndex: number;
  focused: boolean;
  visible: boolean;
  theme: string;
  onToggleTheme: () => void;
  onFileClick: (index: number) => void;
  onFileContextMenu: (e: React.MouseEvent, index: number) => void;
  onFileDrop: (source: { path: string, origin: 'local' | 'backend' }, targetOrigin: 'local' | 'backend') => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  files,
  isBackendAvailable,
  selectedIndex,
  focused,
  visible,
  theme,
  onToggleTheme,
  onFileClick,
  onFileContextMenu,
  onFileDrop
}) => {
  if (!visible) return null;

  const handleDragStart = (e: React.DragEvent, path: string, origin: 'local' | 'backend') => {
    e.dataTransfer.setData('application/json', JSON.stringify({ path, origin }));
    e.dataTransfer.effectAllowed = 'copy';
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  };

  const handleDrop = (e: React.DragEvent, targetOrigin: 'local' | 'backend') => {
    e.preventDefault();
    const data = e.dataTransfer.getData('application/json');
    if (data) {
        try {
            const source = JSON.parse(data);
            if (source.origin !== targetOrigin) {
                onFileDrop(source, targetOrigin);
            }
        } catch (e) {}
    }
  };

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
        {files.map((item, index) => {
          if (item.is_header) {
            return (
              <div
                key={`header-${index}`}
                className="px-2 py-1 text-xs font-bold text-text-muted uppercase tracking-wider mt-2"
              >
                {item.label}
              </div>
            );
          }

          const origin = item.is_app_file ? 'backend' : 'local';
          const indentPx = item.depth * 16;

          return (
            <div
              key={`file-${index}-${item.path}`}
              draggable={!item.is_directory}
              onDragStart={(e) => item.path && handleDragStart(e, item.path, origin)}
              className={clsx(
                "p-2 cursor-pointer flex items-center gap-2 text-sm font-mono hover:bg-bg-tertiary",
                selectedIndex === index && focused ? "bg-selection text-accent" : "text-text-secondary"
              )}
              style={{ paddingLeft: `${8 + indentPx}px` }}
              onClick={() => onFileClick(index)}
              onContextMenu={(e) => {
                e.preventDefault();
                onFileContextMenu(e, index);
              }}
            >
              {item.is_directory ? (
                <>
                  {item.is_expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                  <Folder size={16} />
                </>
              ) : (
                <File size={16} />
              )}
              <span className="truncate">{item.label}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
};
