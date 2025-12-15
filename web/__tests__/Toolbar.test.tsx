import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import { Toolbar } from '../components/Toolbar';

describe('Toolbar', () => {
  it('renders all buttons', () => {
    render(
      <Toolbar 
        onSave={() => {}}
        onNewCell={() => {}}
        onCut={() => {}}
        onCopy={() => {}}
        onPaste={() => {}}
        onRun={() => {}}
        onStop={() => {}}
        onRestart={() => {}}
        onRunAll={() => {}}
      />
    );

    expect(screen.getByTitle(/Save/)).toBeInTheDocument();
    expect(screen.getByTitle(/New Cell/)).toBeInTheDocument();
    expect(screen.getByTitle(/Cut Cell/)).toBeInTheDocument();
    expect(screen.getByTitle(/Copy Cell/)).toBeInTheDocument();
    expect(screen.getByTitle(/Paste Cell/)).toBeInTheDocument();
    expect(screen.getByTitle(/Run Cell/)).toBeInTheDocument();
    expect(screen.getByTitle(/Stop/)).toBeInTheDocument();
    expect(screen.getByTitle(/Restart/)).toBeInTheDocument();
    expect(screen.getByTitle(/Run All/)).toBeInTheDocument();
  });

  it('calls callbacks on click', () => {
    const onSave = jest.fn();
    const onRun = jest.fn();
    
    render(
      <Toolbar 
        onSave={onSave}
        onNewCell={() => {}}
        onCut={() => {}}
        onCopy={() => {}}
        onPaste={() => {}}
        onRun={onRun}
        onStop={() => {}}
        onRestart={() => {}}
        onRunAll={() => {}}
      />
    );

    fireEvent.click(screen.getByTitle(/Save/));
    expect(onSave).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTitle(/Run Cell/));
    expect(onRun).toHaveBeenCalledTimes(1);
  });
});
