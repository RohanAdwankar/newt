import React, { useRef, useEffect } from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { clsx } from 'clsx';
import { PollingStatus } from './PollingStatus';
import { Play, Plus, GripVertical } from 'lucide-react';
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
import { Cell } from './CellList';

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
}) => {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: cell.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    zIndex: isDragging ? 100 : 'auto',
    opacity: isDragging ? 0.5 : 1,
  };

  const activeRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null); // Add textareaRef

  useEffect(() => {
    if (isPrimary && activeRef.current) {
      activeRef.current.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    }
  }, [isPrimary]);

  // Focus logic
  useEffect(() => {
    if (isSelected && editing && textareaRef.current) {
      textareaRef.current.focus();
      const len = textareaRef.current.value.length;
      textareaRef.current.setSelectionRange(len, len);
    }
  }, [isSelected, editing]);

  // Merge refs
  const setRefs = (node: HTMLDivElement | null) => {
    setNodeRef(node);
    // @ts-ignore
    activeRef.current = node;
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
        <div className="flex items-center gap-2">
            {/* Drag Handle */}
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
};
