import React, { useEffect, useRef, useState } from 'react';
import { clsx } from 'clsx';
import { PollingStatus } from './PollingStatus';
import { Play, Plus } from 'lucide-react';
import Editor from 'react-simple-code-editor';
import { highlight, languages } from 'prismjs';
import 'prismjs/components/prism-clike';
import 'prismjs/components/prism-javascript';
import 'prismjs/components/prism-typescript';
import 'prismjs/components/prism-python';
import 'prismjs/components/prism-rust';
import 'prismjs/components/prism-c';
import 'prismjs/components/prism-cpp';
import 'prismjs/components/prism-go';
import 'prismjs/components/prism-bash';

export type CellType = 'rust' | 'python' | 'javascript' | 'typescript' | 'c' | 'cpp' | 'go' | 'shell';

export interface Cell {
  id: string;
  content: string;
  output: string;
  display_data?: { data: Record<string, any>; metadata: Record<string, any> }[];
  type: CellType;
  polling_interval?: number;
  last_run?: number;
}

interface CellListProps {
  cells: Cell[];
  selectedIndices: number[];
  focused: boolean;
  editing: boolean;
  onContentChange: (id: string, content: string) => void;
  onExitEditing: () => void;
  onShiftClick: (e: React.MouseEvent, cellId: string) => void;
  onCellSelect: (index: number, multi: boolean, range: boolean) => void;
  onRangeSelect: (start: number, end: number) => void;
  onRunCell: (index: number) => void;
  onAddCell: (index: number) => void;
  onEditCell: (index: number) => void;
}

export const CellList: React.FC<CellListProps> = ({ 
  cells, 
  selectedIndices, 
  focused, 
  editing, 
  onContentChange, 
  onExitEditing, 
  onShiftClick,
  onCellSelect,
  onRangeSelect,
  onRunCell,
  onAddCell,
  onEditCell
}) => {
  const listRef = useRef<HTMLDivElement>(null);
  const activeRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStartIndex, setDragStartIndex] = useState<number | null>(null);

  // Scroll to active cell (only if single selection changed)
  useEffect(() => {
    if (selectedIndices.length === 1 && activeRef.current) {
      activeRef.current.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    }
  }, [selectedIndices]);

  useEffect(() => {
    if (editing && textareaRef.current) {
      textareaRef.current.focus();
      const len = textareaRef.current.value.length;
      textareaRef.current.setSelectionRange(len, len);
    }
  }, [editing]);

  const handleMouseDown = (index: number, e: React.MouseEvent) => {
    if (editing) return; // Don't interfere with text editing
    
    // If clicking on the run button or add button, don't select
    if ((e.target as HTMLElement).closest('button')) return;

    setIsDragging(true);
    setDragStartIndex(index);
    onCellSelect(index, e.metaKey || e.ctrlKey, e.shiftKey);
  };

  const handleMouseEnter = (index: number) => {
    if (isDragging && dragStartIndex !== null) {
      onRangeSelect(dragStartIndex, index);
    }
  };

  const handleMouseUp = () => {
    setIsDragging(false);
    setDragStartIndex(null);
  };

  useEffect(() => {
    window.addEventListener('mouseup', handleMouseUp);
    return () => window.removeEventListener('mouseup', handleMouseUp);
  }, []);

  return (
    <div className="flex-1 h-full overflow-y-auto p-4 space-y-4 scroll-smooth pb-20 select-none" ref={listRef}>
      {cells.map((cell, index) => {
        const isSelected = selectedIndices.includes(index);
        // We only ref the first selected cell for scrolling
        const isPrimary = selectedIndices.length > 0 && selectedIndices[0] === index;
        
        return (
          <div
            key={cell.id}
            ref={isPrimary ? activeRef : null}
            onMouseDown={(e) => handleMouseDown(index, e)}
            onMouseEnter={() => handleMouseEnter(index)}
            onDoubleClick={(e) => {
                e.stopPropagation();
                onEditCell(index);
            }}
            onClick={(e) => {
                if (e.shiftKey && e.altKey) { 
                    // ...
                }
            }}
            onContextMenu={(e) => {
                e.preventDefault();
                onShiftClick(e, cell.id); 
            }}
            className={clsx(
              "relative rounded border transition-all group",
              isSelected && focused ? "border-accent bg-bg-secondary shadow-[0_0_10px_rgba(34,197,94,0.1)]" : "border-border-color bg-bg-primary"
            )}
          >
            <div 
                className="flex justify-between items-center mb-2 text-xs text-text-muted uppercase tracking-wider select-none p-2 pb-0 cursor-pointer"
                onClick={(e) => {
                    e.stopPropagation();
                    onCellSelect(index, e.metaKey || e.ctrlKey, e.shiftKey);
                }}
            >
              <span className={clsx("font-bold", isSelected && focused ? "text-accent" : "text-text-muted")}>
                [{cell.type}] {cell.id.slice(0, 8)} 
                {cell.polling_interval && (
                    <PollingStatus lastRun={cell.last_run} interval={cell.polling_interval} />
                )}
              </span>
              
              {/* Run Button */}
              <button 
                className="opacity-0 group-hover:opacity-100 transition-opacity p-1 hover:bg-bg-tertiary rounded text-accent"
                onClick={(e) => {
                    e.stopPropagation();
                    onRunCell(index);
                }}
                title="Run Cell"
              >
                <Play size={14} />
              </button>
            </div>
            
            <div className="px-2 pb-2">
                <div 
                    className={clsx(
                        "font-mono text-sm border rounded overflow-hidden",
                        isSelected && editing ? "border-accent" : "border-transparent"
                    )}
                    onKeyDown={(e) => {
                        if (isSelected && editing) {
                            e.stopPropagation(); 
                            if (e.key === 'Escape') {
                                onExitEditing();
                            }
                        }
                    }}
                    onClick={(e) => {
                         if (!editing) {
                             onCellSelect(index, e.metaKey || e.ctrlKey, e.shiftKey);
                         }
                    }}
                >
                    <Editor
                        value={cell.content}
                        onValueChange={(code) => onContentChange(cell.id, code)}
                        highlight={(code) => highlight(code, languages[cell.type === 'shell' ? 'bash' : cell.type] || languages.clike, cell.type)}
                        padding={10}
                        style={{
                            fontFamily: '"Fira Code", "Fira Mono", monospace',
                            fontSize: 14,
                            backgroundColor: isSelected && editing ? 'var(--bg-tertiary)' : 'transparent',
                        }}
                        textareaClassName="focus:outline-none"
                        disabled={!editing}
                    />
                </div>

                {(cell.output || (cell.display_data && cell.display_data.length > 0)) && (
                <div 
                    className="mt-2 border-t border-border-color pt-2 cursor-pointer"
                    onClick={(e) => {
                        e.stopPropagation();
                        onCellSelect(index, e.metaKey || e.ctrlKey, e.shiftKey);
                    }}
                >
                    <div className="text-xs text-text-muted mb-1 select-none">Output:</div>
                    
                    {cell.output && (
                        <pre className="font-mono text-sm text-text-secondary whitespace-pre-wrap bg-bg-tertiary p-2 rounded overflow-x-auto select-text mb-2">
                        {cell.output}
                        </pre>
                    )}

                    {cell.display_data && cell.display_data.map((data, i) => (
                        <div key={i} className="bg-white p-2 rounded overflow-x-auto mb-2">
                            {data.data['image/png'] && (
                                <img src={`data:image/png;base64,${data.data['image/png']}`} alt="Plot" />
                            )}
                            {data.data['image/svg+xml'] && (
                                <div dangerouslySetInnerHTML={{ __html: data.data['image/svg+xml'] }} />
                            )}
                        </div>
                    ))}
                </div>
                )}
            </div>

            {/* Hover to Add Cell Zone */}
            <div 
                className="absolute -bottom-3 left-0 right-0 h-6 flex items-center justify-center opacity-0 hover:opacity-100 z-10 cursor-pointer group/add"
                onClick={(e) => {
                    e.stopPropagation();
                    onAddCell(index + 1);
                }}
            >
                <div className="h-0.5 w-full bg-accent/50 group-hover/add:bg-accent transition-colors absolute top-1/2 transform -translate-y-1/2"></div>
                <div className="bg-bg-primary border border-accent rounded-full p-0.5 text-accent relative z-20 shadow-sm">
                    <Plus size={14} />
                </div>
            </div>
          </div>
        );
      })}
      {cells.length === 0 && (
        <div className="text-center text-text-muted mt-10 italic cursor-pointer hover:text-accent" onClick={() => onAddCell(0)}>
            No cells. Click to add one.
        </div>
      )}
    </div>
  );
};
