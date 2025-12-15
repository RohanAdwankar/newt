import React from 'react';
import { Folder, File, Plus, Sun, Moon } from 'lucide-react';
import { clsx } from 'clsx';

interface SidebarProps {
  localFiles: string[];
  backendFiles: string[];
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
  localFiles,
  backendFiles,
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

  // Flattened index logic:
  // 0: New Notebook (Browser)
  // 1..L: Local Files
  // L+1: New Notebook (Computer)
  // L+2..L+1+B: Backend Files
  
  const computerStartIndex = localFiles.length + 1;

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
        {/* Browser Section */}
        <div 
            onDragOver={handleDragOver}
            onDrop={(e) => handleDrop(e, 'local')}
            className="min-h-[100px]"
        >
            <div className="px-2 py-1 text-xs font-bold text-text-muted uppercase tracking-wider mt-2">Browser</div>
            
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
            
            {localFiles.map((file, index) => (
              <div
                key={`local-${file}`}
                draggable
                onDragStart={(e) => handleDragStart(e, file, 'local')}
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

        {/* Computer Section */}
        {isBackendAvailable && (
          <div
            onDragOver={handleDragOver}
            onDrop={(e) => handleDrop(e, 'backend')}
            className="min-h-[100px]"
          >
            <div className="px-2 py-1 text-xs font-bold text-text-muted uppercase tracking-wider mt-4">Computer</div>
            
            <div 
              className={clsx(
                "p-2 cursor-pointer flex items-center gap-2 text-sm font-mono hover:bg-bg-tertiary",
                selectedIndex === computerStartIndex && focused ? "bg-selection text-accent" : "text-text-secondary"
              )}
              onClick={() => onFileClick(computerStartIndex)}
              onContextMenu={(e) => {
                e.preventDefault();
                onFileContextMenu(e, computerStartIndex);
              }}
            >
              <Plus size={16} />
              <span>New Notebook</span>
            </div>

            {backendFiles.map((file, index) => (
              <div
                key={`backend-${file}`}
                draggable
                onDragStart={(e) => handleDragStart(e, file, 'backend')}
                className={clsx(
                  "p-2 cursor-pointer flex items-center gap-2 truncate text-sm font-mono hover:bg-bg-tertiary",
                  selectedIndex === computerStartIndex + 1 + index && focused ? "bg-selection text-accent" : "text-text-secondary"
                )}
                onClick={() => onFileClick(computerStartIndex + 1 + index)}
                onContextMenu={(e) => {
                    e.preventDefault();
                    onFileContextMenu(e, computerStartIndex + 1 + index);
                }}
              >
                <File size={16} />
                <span>{file}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
