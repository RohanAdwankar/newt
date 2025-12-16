import React, { useRef, useEffect } from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { clsx } from 'clsx';
import { PollingStatus } from './PollingStatus';
import { Play, Plus, GripVertical } from 'lucide-react';
import CodeMirror, { ReactCodeMirrorRef } from '@uiw/react-codemirror';
import { vim } from '@replit/codemirror-vim';
import { python } from '@codemirror/lang-python';
import { rust } from '@codemirror/lang-rust';
import { javascript } from '@codemirror/lang-javascript';
import { cpp } from '@codemirror/lang-cpp';
import { go } from '@codemirror/lang-go';
import { EditorView } from '@codemirror/view';
import { Cell } from './CellList';

// Remove prismjs imports as we are using CodeMirror
// import Editor from 'react-simple-code-editor';
// import { highlight, languages } from 'prismjs';
// ...

interface SortableCellProps {
  cell: Cell;
  index: number;
  isSelected: boolean;
  isPrimary: boolean;
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
  // Selection drag handlers from parent
  onSelectionMouseDown: (index: number, e: React.MouseEvent) => void;
  onSelectionMouseEnter: (index: number) => void;
  isFullscreen?: boolean;
  vimMode: boolean;
  theme: string;
}

export const SortableCell: React.FC<SortableCellProps> = ({
  cell,
  index,
  isSelected,
  isPrimary,
  focused,
  editing,
  onContentChange,
  onExitEditing,
  onShiftClick,
  onCellSelect,
  onRangeSelect,
  onRunCell,
  onAddCell,
  onEditCell,
  onSelectionMouseDown,
  onSelectionMouseEnter,
  isFullscreen,
  vimMode,
  theme
}) => {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: cell.id, disabled: isFullscreen });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    zIndex: isDragging ? 100 : 'auto',
    opacity: isDragging ? 0.5 : 1,
    height: isFullscreen ? '100%' : 'auto',
  };

  const activeRef = useRef<HTMLDivElement>(null);
  const cmRef = useRef<ReactCodeMirrorRef>(null);

  useEffect(() => {
    if (isPrimary && activeRef.current) {
      activeRef.current.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    }
  }, [isPrimary]);

  // Focus logic
  useEffect(() => {
    if (isSelected && editing && cmRef.current?.view) {
        cmRef.current.view.focus();
    }
  }, [isSelected, editing]);
  
  // Merge refs
  const setRefs = (node: HTMLDivElement | null) => {
    setNodeRef(node);
    // @ts-ignore
    activeRef.current = node;
  };

  const getExtensions = () => {
    const exts = [EditorView.lineWrapping];
    if (vimMode && editing && isSelected) exts.push(vim());
    
    switch (cell.type) {
        case 'python': exts.push(python()); break;
        case 'rust': exts.push(rust()); break;
        case 'javascript': 
        case 'typescript': exts.push(javascript({ typescript: true })); break;
        case 'c':
        case 'cpp': exts.push(cpp()); break;
        case 'go': exts.push(go()); break;
        // shell not supported directly in basic packages, fallback to plain text or simple highlighting if available
    }
    
    // Custom keymap to handle Escape in Normal mode to exit editing
    if (vimMode && editing && isSelected) {
        exts.push(EditorView.domEventHandlers({
            keydown: (e, view) => {
                // @ts-ignore
                const cm = view.cm; // vim extension attaches cm instance
                if (e.key === 'Escape' && cm) {
                    // Check if in normal mode
                    // The vim extension exposes state via cm.state.vim
                    // But it's tricky to access.
                    // Alternatively, if we are in normal mode, 'Escape' usually does nothing or clears selection.
                    // If we press Escape and we are NOT in insert mode, we want to exit editing.
                    
                    // @ts-ignore
                    const state = cm.state.vim;
                    if (state && !state.insertMode && !state.visualMode) {
                        onExitEditing();
                        return true;
                    }
                }
                return false;
            }
        }));
    } else {
        // Standard escape to exit
        exts.push(EditorView.domEventHandlers({
            keydown: (e) => {
                if (e.key === 'Escape') {
                    onExitEditing();
                    return true;
                }
                return false;
            }
        }));
    }

    return exts;
  };

  return (
    <div
      ref={setRefs}
      style={style}
      onMouseDown={(e) => onSelectionMouseDown(index, e)}
      onMouseEnter={() => onSelectionMouseEnter(index)}
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
        "relative rounded border transition-all group flex flex-col outline-none",
        isSelected && focused 
            ? (editing ? "border-accent bg-bg-secondary shadow-[0_0_10px_rgba(34,197,94,0.1)]" : "border-text-muted bg-bg-secondary") 
            : "border-border-color bg-bg-primary",
        isFullscreen && "h-full"
      )}
    >
      <div
        className="flex justify-between items-center mb-2 text-xs text-text-muted uppercase tracking-wider select-none p-2 pb-0 cursor-pointer shrink-0"
        onClick={(e) => {
          e.stopPropagation();
          onCellSelect(index, e.metaKey || e.ctrlKey, e.shiftKey);
        }}
      >
        <div className="flex items-center gap-2">
            {/* Drag Handle */}
            {!isFullscreen && (
            <div 
                className="cursor-grab active:cursor-grabbing hover:text-text-primary"
                {...attributes} 
                {...listeners}
                onMouseDown={(e) => {
                    // Prevent selection logic when dragging
                    e.stopPropagation();
                    listeners?.onMouseDown?.(e);
                }}
            >
                <GripVertical size={14} />
            </div>
            )}

            <span className={clsx("font-bold", isSelected && focused ? "text-accent" : "text-text-muted")}>
            [{cell.type}] {cell.id.slice(0, 8)}
            {cell.polling_interval && (
                <PollingStatus lastRun={cell.last_run} interval={cell.polling_interval} />
            )}
            </span>
        </div>

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

      <div className={clsx("px-2 pb-2 flex flex-col", isFullscreen ? "flex-1 overflow-hidden" : "")}>
        <div
          className={clsx(
            "font-mono text-sm border rounded overflow-hidden",
            isSelected && editing ? "border-accent" : "border-transparent",
            isFullscreen ? "flex-1 overflow-y-auto" : ""
          )}
          onClick={(e) => {
            if (!editing) {
              onCellSelect(index, e.metaKey || e.ctrlKey, e.shiftKey);
            }
          }}
        >
          <CodeMirror
            ref={cmRef}
            value={cell.content}
            height={isFullscreen ? "100%" : "auto"}
            extensions={getExtensions()}
            onChange={(value) => onContentChange(cell.id, value)}
            theme={theme === 'dark' ? 'dark' : 'light'}
            editable={editing && isSelected}
            basicSetup={{
                lineNumbers: false,
                foldGutter: false,
                highlightActiveLine: false,
                highlightActiveLineGutter: false,
            }}
            className="cm-editor-custom focus:outline-none"
          />
        </div>

        {(cell.output || (cell.display_data && cell.display_data.length > 0)) && (
          <div
            className={clsx(
                "mt-2 border-t border-border-color pt-2 cursor-pointer",
                isFullscreen ? "max-h-[40%] overflow-y-auto shrink-0" : ""
            )}
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
      {!isFullscreen && (
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
      )}
    </div>
  );
};
