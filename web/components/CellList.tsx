import React, { useEffect, useRef, useState } from 'react';
import {
  DndContext, 
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  DragEndEvent
} from '@dnd-kit/core';
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { SortableCell } from './SortableCell';

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
  onReorder: (oldIndex: number, newIndex: number) => void;
  fullscreenCellId: string | null;
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
  onEditCell,
  onReorder,
  fullscreenCellId
}) => {
  const listRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStartIndex, setDragStartIndex] = useState<number | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;

    if (over && active.id !== over.id) {
      const oldIndex = cells.findIndex((cell) => cell.id === active.id);
      const newIndex = cells.findIndex((cell) => cell.id === over.id);
      onReorder(oldIndex, newIndex);
    }
  };

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

  if (fullscreenCellId) {
    const cell = cells.find(c => c.id === fullscreenCellId);
    const index = cells.findIndex(c => c.id === fullscreenCellId);
    if (cell && index !== -1) {
        return (
            <div className="flex-1 h-full overflow-hidden p-4">
                 <SortableCell
                    key={cell.id}
                    cell={cell}
                    index={index}
                    isSelected={true}
                    isPrimary={true}
                    focused={focused}
                    editing={editing}
                    onContentChange={onContentChange}
                    onExitEditing={onExitEditing}
                    onShiftClick={onShiftClick}
                    onCellSelect={onCellSelect}
                    onRangeSelect={onRangeSelect}
                    onRunCell={onRunCell}
                    onAddCell={onAddCell}
                    onEditCell={onEditCell}
                    onSelectionMouseDown={() => {}}
                    onSelectionMouseEnter={() => {}}
                    isFullscreen={true}
                  />
            </div>
        )
    }
  }

  return (
    <DndContext 
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragEnd={handleDragEnd}
    >
      <div className="flex-1 h-full overflow-y-auto p-4 space-y-4 scroll-smooth pb-20 select-none" ref={listRef}>
        <SortableContext 
          items={cells.map(c => c.id)}
          strategy={verticalListSortingStrategy}
        >
          {cells.map((cell, index) => {
            const isSelected = selectedIndices.includes(index);
            const isPrimary = selectedIndices.length > 0 && selectedIndices[0] === index;
            
            return (
              <SortableCell
                key={cell.id}
                cell={cell}
                index={index}
                isSelected={isSelected}
                isPrimary={isPrimary}
                focused={focused}
                editing={editing}
                onContentChange={onContentChange}
                onExitEditing={onExitEditing}
                onShiftClick={onShiftClick}
                onCellSelect={onCellSelect}
                onRangeSelect={onRangeSelect}
                onRunCell={onRunCell}
                onAddCell={onAddCell}
                onEditCell={onEditCell}
                onSelectionMouseDown={handleMouseDown}
                onSelectionMouseEnter={handleMouseEnter}
              />
            );
          })}
        </SortableContext>
        {cells.length === 0 && (
          <div className="text-center text-text-muted mt-10 italic cursor-pointer hover:text-accent" onClick={() => onAddCell(0)}>
              No cells. Click to add one.
          </div>
        )}
      </div>
    </DndContext>
  );
};
