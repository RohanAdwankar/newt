"use client";

import React, { useState, useEffect, useCallback } from 'react';
import { Sidebar } from '../components/Sidebar';
import { CellList, Cell, CellType } from '../components/CellList';
import { CommandBar } from '../components/CommandBar';
import { Toolbar } from '../components/Toolbar';
import { v4 as uuidv4 } from 'uuid';
import { createKernel, ExecutionMode } from '../lib/kernels';

const API_URL = 'http://127.0.0.1:3000';

type Focus = 'editor' | 'sidebar';
type InputMode = 'normal' | 'editing' | 'command' | 'renaming' | 'polling';

export default function App() {
  const [cells, setCells] = useState<Cell[]>([]);
  const [files, setFiles] = useState<string[]>([]);
  const [focus, setFocus] = useState<Focus>('editor');
  const [inputMode, setInputMode] = useState<InputMode>('normal');
  const [selectedIndices, setSelectedIndices] = useState<number[]>([0]);
  const [selectedFileIndex, setSelectedFileIndex] = useState(0);
  const [commandInput, setCommandInput] = useState('');
  const [pollingInput, setPollingInput] = useState('');
  const [contextMenu, setContextMenu] = useState<{ x: number, y: number, cellId: string } | null>(null);
  const [fileContextMenu, setFileContextMenu] = useState<{ x: number, y: number, index: number } | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [filePath, setFilePath] = useState<string | null>(null);
  const [clipboardCells, setClipboardCells] = useState<Cell[]>([]);
  const [clipboardFile, setClipboardFile] = useState<string | null>(null);
  const [showSidebar, setShowSidebar] = useState(false);
  const [renameInput, setRenameInput] = useState('');
  const [theme, setTheme] = useState('dark');
  const [executionMode, setExecutionMode] = useState<ExecutionMode>('client');

  // Helper to get primary selection (last selected)
  const primaryIndex = selectedIndices.length > 0 ? selectedIndices[selectedIndices.length - 1] : 0;

  const toggleTheme = async () => {
    const newTheme = theme === 'dark' ? 'light' : 'dark';
    setTheme(newTheme);
    document.documentElement.setAttribute('data-theme', newTheme);
    
    try {
      await fetch(`${API_URL}/config`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ theme: newTheme })
      });
    } catch (e) {
      console.error("Failed to save config", e);
    }
  };

  const fetchConfig = useCallback(async () => {
    try {
      const res = await fetch(`${API_URL}/config`);
      if (res.ok) {
        const config = await res.json();
        if (config.theme) {
          setTheme(config.theme);
          document.documentElement.setAttribute('data-theme', config.theme);
        }
      }
    } catch (e) {
      console.warn("Failed to fetch config (backend might be offline)", e);
    }
  }, []);

  const handlePollingInput = (input: string) => {
    if (cells.length === 0) return;
    let interval: number | undefined;

    if (input === "r/") {
        interval = undefined;
        setStatusMessage("Polling disabled");
    } else if (input.startsWith("rm")) {
        const val = parseInt(input.slice(2));
        if (!isNaN(val)) {
            interval = val * 60;
            setStatusMessage(`Polling set to ${interval}s`);
        }
    } else if (input.startsWith("rh")) {
        const val = parseInt(input.slice(2));
        if (!isNaN(val)) {
            interval = val * 3600;
            setStatusMessage(`Polling set to ${interval}s`);
        }
    } else if (input.startsWith("r")) {
        const val = parseInt(input.slice(1));
        if (!isNaN(val)) {
            interval = val;
            setStatusMessage(`Polling set to ${interval}s`);
        }
    }

    setCells(prev => {
        const newCells = [...prev];
        // Apply to all selected cells? Or just primary? Let's do primary for now as polling is per cell
        newCells[primaryIndex] = { ...newCells[primaryIndex], polling_interval: interval };
        return newCells;
    });
  };

  const handleShiftClick = (e: React.MouseEvent, cellId: string) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY, cellId });
  };

  const handleCellSelect = (index: number, multi: boolean, range: boolean) => {
    setFocus('editor');
    if (range) {
        // Range selection from primaryIndex to index
        const start = Math.min(primaryIndex, index);
        const end = Math.max(primaryIndex, index);
        const rangeIndices = [];
        for (let i = start; i <= end; i++) rangeIndices.push(i);
        setSelectedIndices(rangeIndices);
    } else if (multi) {
        // Toggle selection
        setSelectedIndices(prev => {
            if (prev.includes(index)) {
                return prev.filter(i => i !== index);
            } else {
                return [...prev, index];
            }
        });
    } else {
        // Single selection
        setSelectedIndices([index]);
    }
  };

  const handleRangeSelect = (start: number, end: number) => {
    const s = Math.min(start, end);
    const e = Math.max(start, end);
    const indices = [];
    for (let i = s; i <= e; i++) indices.push(i);
    setSelectedIndices(indices);
  };

  const handleFileClick = (index: number) => {
    setSelectedFileIndex(index);
    openFile(index);
  };

  const handleFileContextMenu = (e: React.MouseEvent, index: number) => {
    e.preventDefault();
    setSelectedFileIndex(index);
    setFileContextMenu({ x: e.clientX, y: e.clientY, index });
  };

  // Fetch files on mount
  const fetchFiles = useCallback(async () => {
    try {
      const res = await fetch(`${API_URL}/files`);
      if (!res.ok) throw new Error(`HTTP error! status: ${res.status}`);
      const data = await res.json();
      setFiles(data);
    } catch (e) {
      console.warn("Failed to fetch files (backend might be offline)", e);
      setFiles([]); // Set empty list on error
    }
  }, []);

  useEffect(() => {
    fetchFiles();
    fetchConfig();
  }, [fetchFiles, fetchConfig]);

  // Key handler
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (inputMode === 'editing') {
        return;
      }

      if (inputMode === 'command') {
        if (e.key === 'Enter') {
          e.preventDefault();
          await executeCommand(commandInput);
          setInputMode('normal');
          setCommandInput('');
          setStatusMessage(null);
        } else if (e.key === 'Escape') {
          setInputMode('normal');
          setCommandInput('');
          setStatusMessage(null);
        } else if (e.key === 'Backspace') {
          setCommandInput(prev => prev.slice(0, -1));
        } else if (e.key.length === 1) {
          setCommandInput(prev => prev + e.key);
        }
        return;
      }

      if (inputMode === 'polling') {
        if (e.key === 'Enter') {
          e.preventDefault();
          handlePollingInput(pollingInput);
          setInputMode('normal');
          setPollingInput('');
        } else if (e.key === 'Escape') {
          setInputMode('normal');
          setPollingInput('');
        } else if (e.key === 'Backspace') {
          setPollingInput(prev => prev.slice(0, -1));
        } else if (e.key.length === 1) {
          setPollingInput(prev => prev + e.key);
        }
        return;
      }

      if (inputMode === 'renaming') {
         if (e.key === 'Enter') {
            e.preventDefault();
            await handleRename();
            setInputMode('normal');
         } else if (e.key === 'Escape') {
            setInputMode('normal');
         } else if (e.key === 'Backspace') {
            setRenameInput(prev => prev.slice(0, -1));
         } else if (e.key.length === 1) {
            setRenameInput(prev => prev + e.key);
         }
         return;
      }

      // Normal Mode
      if (focus === 'editor') {
        switch (e.key) {
          case 'j':
            setSelectedIndices(prev => {
                const last = prev[prev.length - 1];
                const next = Math.min(last + 1, cells.length - 1);
                return [next];
            });
            break;
          case 'k':
            setSelectedIndices(prev => {
                const last = prev[prev.length - 1];
                const next = Math.max(last - 1, 0);
                return [next];
            });
            break;
          case 'h':
          case 'ArrowLeft':
            if (showSidebar) setFocus('sidebar');
            break;
          case 'i':
            if (cells.length > 0) setInputMode('editing');
            break;
          case 'r':
            setInputMode('polling');
            setPollingInput('r');
            break;
          case 'Enter':
            if (cells.length > 0) runCell(primaryIndex);
            break;
          case ':':
            setInputMode('command');
            setStatusMessage(null);
            break;
          case 'y':
            copyCells();
            break;
          case 'p':
            pasteCells(true); // below
            break;
          case 'P':
            pasteCells(false); // above
            break;
          case 'o':
            addCell(primaryIndex + 1);
            break;
          case 'O':
            addCell(primaryIndex);
            break;
          case 'd':
            deleteCells();
            break;
        }
      } else if (focus === 'sidebar') {
        switch (e.key) {
          case 'j':
            setSelectedFileIndex(prev => Math.min(prev + 1, files.length));
            break;
          case 'k':
            setSelectedFileIndex(prev => Math.max(prev - 1, 0));
            break;
          case 'l':
          case 'ArrowRight':
            setFocus('editor');
            break;
          case 'Enter':
            openFile(selectedFileIndex);
            break;
          case 'r':
            if (selectedFileIndex > 0) {
                setRenameInput(files[selectedFileIndex - 1]);
                setInputMode('renaming');
            }
            break;
          case 'y':
            if (selectedFileIndex > 0) {
                setClipboardFile(files[selectedFileIndex - 1]);
                setStatusMessage(`Yanked ${files[selectedFileIndex - 1]}`);
            }
            break;
          case 'p':
          case 'P':
            if (clipboardFile) {
                await pasteFile(clipboardFile);
            }
            break;
          case ':':
            setInputMode('command');
            setStatusMessage(null);
            break;
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [cells, files, focus, inputMode, selectedIndices, selectedFileIndex, commandInput, pollingInput, clipboardCells, clipboardFile, showSidebar, renameInput]);

  // Space chord handler
  useEffect(() => {
    let spacePressed = false;
    const handleKeyDown = (e: KeyboardEvent) => {
        if (inputMode !== 'normal') return;
        
        if (e.key === ' ') {
            spacePressed = true;
        } else if (spacePressed) {
            if (e.key === 'e') {
                setShowSidebar(prev => !prev);
            } else if (e.key === 'h' || e.key === 'ArrowLeft') {
                setFocus('sidebar');
            } else if (e.key === 'l' || e.key === 'ArrowRight') {
                setFocus('editor');
            }
            spacePressed = false;
        }
    };
    const handleKeyUp = (e: KeyboardEvent) => {
        if (e.key === ' ') spacePressed = false;
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
        window.removeEventListener('keydown', handleKeyDown);
        window.removeEventListener('keyup', handleKeyUp);
    };
  }, [inputMode]);


  const executeCommand = async (cmd: string) => {
    if (cmd === 'ra' || cmd === 'runall') {
        runAllCells();
    } else if (cmd === 'export') {
        await exportNotebook();
    } else if (cmd === 'w') {
        await saveNotebook();
    } else if (cmd === 'q') {
        // Close?
    }
  };

  const runCell = async (index: number) => {
    const cell = cells[index];
    if (!cell) return;

    // Check for language change
    const content = cell.content.trim();
    const langMap: Record<string, CellType> = {
        'rust': 'rust',
        'python': 'python',
        'py': 'python',
        'javascript': 'javascript',
        'js': 'javascript',
        'typescript': 'typescript',
        'ts': 'typescript',
        'c': 'c',
        'cpp': 'cpp',
        'c++': 'cpp',
        'go': 'go',
        'shell': 'shell',
        'sh': 'shell'
    };

    if (langMap[content]) {
        setCells(prev => {
            const newCells = [...prev];
            newCells[index] = { ...newCells[index], type: langMap[content], content: '' };
            return newCells;
        });
        setStatusMessage(`Changed cell type to ${langMap[content]}`);
        return;
    }

    let context: string[] = [];
    for (let i = 0; i < index; i++) {
        if (cells[i].type === cell.type && cell.type !== 'shell') {
            context.push(cells[i].content);
        }
    }

    try {
        const kernel = createKernel(executionMode);
        const result = await kernel.execute(cell.content, cell.type, context);
        
        setCells(prev => {
            const newCells = [...prev];
            newCells[index] = { 
                ...newCells[index], 
                output: result.stdout + result.stderr, 
                display_data: result.display_data 
            };
            return newCells;
        });
    } catch (e) {
        setCells(prev => {
            const newCells = [...prev];
            newCells[index] = { ...newCells[index], output: "Error connecting to kernel" };
            return newCells;
        });
    }
  };

  const runAllCells = async () => {
    for (let i = 0; i < cells.length; i++) {
        await runCell(i);
    }
  };

  const addCell = (index: number) => {
    const newCell: Cell = { id: uuidv4(), content: '', output: '', type: 'python' };
    setCells(prev => {
        const newCells = [...prev];
        newCells.splice(index, 0, newCell);
        return newCells;
    });
    setSelectedIndices([index]);
    setInputMode('editing');
  };

  const deleteCells = () => {
    setCells(prev => prev.filter((_, i) => !selectedIndices.includes(i)));
    // Reset selection to something safe
    const min = Math.min(...selectedIndices);
    setSelectedIndices([Math.max(0, min - 1)]);
  };

  const copyCells = () => {
    const selected = cells.filter((_, i) => selectedIndices.includes(i));
    setClipboardCells(selected);
    setStatusMessage(`${selected.length} cells yanked`);
  };

  const pasteCells = (below: boolean) => {
    if (clipboardCells.length === 0) return;
    
    const insertIndex = below ? primaryIndex + 1 : primaryIndex;
    const newCells = clipboardCells.map(c => ({ ...c, id: uuidv4() }));
    
    setCells(prev => {
        const next = [...prev];
        next.splice(insertIndex, 0, ...newCells);
        return next;
    });
    
    // Select pasted cells
    const newIndices = [];
    for (let i = 0; i < newCells.length; i++) {
        newIndices.push(insertIndex + i);
    }
    setSelectedIndices(newIndices);
    setStatusMessage(`${newCells.length} cells pasted`);
  };

  const openFile = async (index: number) => {
    if (index === 0) {
        // New Notebook
        setCells([{ id: uuidv4(), content: '', output: '', type: 'python' }]);
        setFilePath(null);
        setFocus('editor');
        setSelectedIndices([0]);
    } else {
        const file = files[index - 1];
        try {
            const res = await fetch(`${API_URL}/files/read`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: file })
            });
            const content = await res.json();
            try {
                const loadedCells = JSON.parse(content);
                setCells(loadedCells);
                setFilePath(file);
                setFocus('editor');
                setSelectedIndices([0]);
            } catch (e) {
                setStatusMessage("Error parsing notebook");
            }
        } catch (e) {
            setStatusMessage("Error reading file");
        }
    }
  };

  const saveNotebook = async () => {
    if (!filePath) {
        setStatusMessage("No file name");
        return;
    }
    try {
        await fetch(`${API_URL}/files/save`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ path: filePath, content: JSON.stringify(cells) })
        });
        setStatusMessage(`Saved to ${filePath}`);
        fetchFiles();
    } catch (e) {
        setStatusMessage("Save failed");
    }
  };

  const handleRename = async () => {
    if (selectedFileIndex > 0) {
        const oldPath = files[selectedFileIndex - 1];
        const newPath = renameInput;
        try {
            await fetch(`${API_URL}/files/rename`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ old_path: oldPath, new_path: newPath })
            });
            fetchFiles();
            setStatusMessage(`Renamed to ${newPath}`);
        } catch (e) {
            setStatusMessage("Rename failed");
        }
    }
  };

  const pasteFile = async (src: string) => {
    let dest = src;
    dest = dest + "_copy"; 
    
    try {
        await fetch(`${API_URL}/files/copy`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ src, dest })
        });
        fetchFiles();
        setStatusMessage(`Pasted to ${dest}`);
    } catch (e) {
        setStatusMessage("Paste failed");
    }
  };

  const deleteFile = async (filename: string) => {
    try {
        await fetch(`${API_URL}/files/delete`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ path: filename })
        });
        fetchFiles();
        setStatusMessage(`Deleted ${filename}`);
    } catch (e) {
        setStatusMessage("Delete failed");
    }
  };

  const exportNotebook = async () => {
    try {
        const res = await fetch(`${API_URL}/export`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ cells })
        });
        const data = await res.json();
        if (filePath) {
            const mdPath = filePath.replace('.json', '.md');
            await fetch(`${API_URL}/files/save`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: mdPath, content: data.markdown })
            });
            setStatusMessage(`Exported to ${mdPath}`);
        }
    } catch (e) {
        setStatusMessage("Export failed");
    }
  };

  useEffect(() => {
    const interval = setInterval(() => {
        const now = Math.floor(Date.now() / 1000);
        const toRun: number[] = [];
        
        cells.forEach((cell, index) => {
            if (cell.polling_interval) {
                const last = cell.last_run || 0;
                if (now >= last + cell.polling_interval) {
                    toRun.push(index);
                }
            }
        });

        if (toRun.length > 0) {
            setCells(prev => {
                const newCells = [...prev];
                toRun.forEach(i => {
                    newCells[i] = { ...newCells[i], last_run: now };
                });
                return newCells;
            });
            
            toRun.forEach(i => runCell(i));
        }
    }, 1000);
    return () => clearInterval(interval);
  }, [cells]);

  return (
    <div className="flex flex-col h-screen w-screen bg-bg-primary text-text-primary overflow-hidden">
      <Toolbar 
        onSave={saveNotebook}
        onNewCell={() => addCell(primaryIndex + 1)}
        onCut={() => { copyCells(); deleteCells(); }}
        onCopy={copyCells}
        onPaste={() => pasteCells(true)}
        onRun={() => selectedIndices.forEach(i => runCell(i))}
        onStop={() => {}}
        onRestart={() => {}}
        onRunAll={runAllCells}
        executionMode={executionMode}
        onExecutionModeChange={setExecutionMode}
      />
      <div className="flex-1 flex overflow-hidden">
        <Sidebar 
            files={files} 
            selectedIndex={selectedFileIndex} 
            focused={focus === 'sidebar'} 
            visible={showSidebar}
            theme={theme}
            onToggleTheme={toggleTheme}
            onFileClick={handleFileClick}
            onFileContextMenu={handleFileContextMenu}
        />
        <CellList 
            cells={cells} 
            selectedIndices={selectedIndices} 
            focused={focus === 'editor'} 
            editing={inputMode === 'editing'}
            onContentChange={(id, content) => {
                setCells(prev => prev.map(c => c.id === id ? { ...c, content } : c));
            }}
            onExitEditing={() => setInputMode('normal')}
            onShiftClick={handleShiftClick}
            onCellSelect={handleCellSelect}
            onRangeSelect={handleRangeSelect}
            onRunCell={runCell}
            onAddCell={addCell}
            onEditCell={() => setInputMode('editing')}
        />
      </div>
      
      {contextMenu && (
        <div 
            className="fixed bg-bg-secondary border border-border-color p-2 rounded shadow-lg z-50"
            style={{ top: contextMenu.y, left: contextMenu.x }}
        >
            <div className="text-sm mb-2">Polling Interval (s):</div>
            <input 
                type="number" 
                className="bg-bg-primary border border-border-color p-1 rounded w-full mb-2"
                onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                        const val = parseInt((e.target as HTMLInputElement).value);
                        setCells(prev => prev.map(c => c.id === contextMenu.cellId ? { ...c, polling_interval: isNaN(val) || val <= 0 ? undefined : val } : c));
                        setContextMenu(null);
                    }
                }}
                autoFocus
            />
            <button 
                className="bg-red-500 text-white px-2 py-1 rounded text-xs w-full"
                onClick={() => {
                    setCells(prev => prev.map(c => c.id === contextMenu.cellId ? { ...c, polling_interval: undefined } : c));
                    setContextMenu(null);
                }}
            >
                Disable Polling
            </button>
            <div className="fixed inset-0 -z-10" onClick={() => setContextMenu(null)} />
        </div>
      )}

      {fileContextMenu && (
        <div 
            className="fixed bg-bg-secondary border border-border-color p-2 rounded shadow-lg z-50"
            style={{ top: fileContextMenu.y, left: fileContextMenu.x }}
        >
            <div 
                className="p-1 hover:bg-bg-tertiary cursor-pointer text-sm"
                onClick={() => {
                    if (fileContextMenu.index > 0) {
                        setRenameInput(files[fileContextMenu.index - 1]);
                        setInputMode('renaming');
                    }
                    setFileContextMenu(null);
                }}
            >
                Rename
            </div>
            <div 
                className="p-1 hover:bg-bg-tertiary cursor-pointer text-sm"
                onClick={() => {
                    if (fileContextMenu.index > 0) {
                        setClipboardFile(files[fileContextMenu.index - 1]);
                        setStatusMessage(`Yanked ${files[fileContextMenu.index - 1]}`);
                    }
                    setFileContextMenu(null);
                }}
            >
                Copy
            </div>
            <div 
                className="p-1 hover:bg-bg-tertiary cursor-pointer text-sm"
                onClick={() => {
                    if (clipboardFile) {
                        pasteFile(clipboardFile);
                    }
                    setFileContextMenu(null);
                }}
            >
                Paste
            </div>
            <div 
                className="p-1 hover:bg-bg-tertiary cursor-pointer text-sm text-red-500"
                onClick={() => {
                    if (fileContextMenu.index > 0) {
                        deleteFile(files[fileContextMenu.index - 1]);
                    }
                    setFileContextMenu(null);
                }}
            >
                Delete
            </div>
            <div className="fixed inset-0 -z-10" onClick={() => setFileContextMenu(null)} />
        </div>
      )}

      {inputMode === 'renaming' && (
        <div className="absolute inset-0 flex items-center justify-center bg-black/50 z-50">
            <div className="bg-bg-secondary border border-accent p-4 rounded w-96">
                <div className="text-accent font-bold mb-2">Rename File</div>
                <input 
                    className="w-full bg-bg-tertiary text-text-primary p-2 outline-none border border-border-color focus:border-accent"
                    value={renameInput}
                    onChange={(e) => setRenameInput(e.target.value)}
                    autoFocus
                />
            </div>
        </div>
      )}

      <CommandBar 
        mode={inputMode} 
        commandInput={commandInput} 
        pollingInput={pollingInput}
        statusMessage={statusMessage} 
        filePath={filePath}
      />
    </div>
  );
}

