import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import { CellList, Cell } from '../components/CellList';

describe('CellList', () => {
  const mockCells: Cell[] = [
    { id: '1', content: 'print("hello")', output: '', type: 'python' },
    { id: '2', content: 'fn main() {}', output: '', type: 'rust' }
  ];

  beforeAll(() => {
    // Mock scrollIntoView
    window.HTMLElement.prototype.scrollIntoView = jest.fn();
  });

  it('renders cells', () => {
    render(
      <CellList 
        cells={mockCells}
        selectedIndices={[0]}
        focused={true}
        editing={false}
        onContentChange={() => {}}
        onExitEditing={() => {}}
        onShiftClick={() => {}}
        onCellSelect={() => {}}
        onRangeSelect={() => {}}
        onRunCell={() => {}}
        onAddCell={() => {}}
        onEditCell={() => {}}
      />
    );

    expect(screen.getByText((_, element) => element?.textContent === 'print("hello")' && element?.classList.contains('cm-line'))).toBeInTheDocument();
    expect(screen.getByText((_, element) => element?.textContent === 'fn main() {}' && element?.classList.contains('cm-line'))).toBeInTheDocument();
  });

  it('calls onCellSelect when clicked', () => {
    const onCellSelect = jest.fn();
    render(
      <CellList 
        cells={mockCells}
        selectedIndices={[0]}
        focused={true}
        editing={false}
        onContentChange={() => {}}
        onExitEditing={() => {}}
        onShiftClick={() => {}}
        onCellSelect={onCellSelect}
        onRangeSelect={() => {}}
        onRunCell={() => {}}
        onAddCell={() => {}}
        onEditCell={() => {}}
      />
    );

    // Click on the second cell's content (pre tag)
    fireEvent.click(screen.getByText((_, element) => element?.textContent === 'fn main() {}' && element?.classList.contains('cm-line')));
    expect(onCellSelect).toHaveBeenCalledWith(1, false, false);
  });

  it('calls onEditCell when double clicked', () => {
    const onEditCell = jest.fn();
    render(
      <CellList 
        cells={mockCells}
        selectedIndices={[0]}
        focused={true}
        editing={false}
        onContentChange={() => {}}
        onExitEditing={() => {}}
        onShiftClick={() => {}}
        onCellSelect={() => {}}
        onRangeSelect={() => {}}
        onRunCell={() => {}}
        onAddCell={() => {}}
        onEditCell={onEditCell}
      />
    );

    fireEvent.doubleClick(screen.getByText((_, element) => element?.textContent === 'print("hello")' && element?.classList.contains('cm-line')));
    expect(onEditCell).toHaveBeenCalledWith(0);
  });
});
