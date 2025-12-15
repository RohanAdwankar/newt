"use client";

import React, { useState, useEffect, useCallback, useRef } from 'react';
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
  const [localFiles, setLocalFiles] = useState<string[]>([]);
  const [backendFiles, setBackendFiles] = useState<string[]>([]);
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
  const [fileOrigin, setFileOrigin] = useState<'local' | 'backend'>('local');
  const [clipboardCells, setClipboardCells] = useState<Cell[]>([]);
  const [clipboardFile, setClipboardFile] = useState<{ path: string, origin: 'local' | 'backend' } | null>(null);
  const [showSidebar, setShowSidebar] = useState(false);
  const [renameInput, setRenameInput] = useState('');
  const [theme, setTheme] = useState('dark');
  const [executionMode, setExecutionMode] = useState<ExecutionMode>('client');
  const [isBackendAvailable, setIsBackendAvailable] = useState(false);
  const [vimMode, setVimMode] = useState(true);
  const spacePressedRef = useRef(false);

  // Helper to get primary selection (last selected)
  const primaryIndex = selectedIndices.length > 0 ? selectedIndices[selectedIndices.length - 1] : 0;

  // Local Storage FS Helpers
  const getLocalFiles = useCallback((): string[] => {
    try {
        return JSON.parse(localStorage.getItem('newt_files_index') || '[]');
    } catch {
        return [];
    }
  }, []);

  const saveLocalFile = useCallback((path: string, content: string) => {
    localStorage.setItem('newt_file:' + path, content);
    const files = getLocalFiles();
    if (!files.includes(path)) {
        files.push(path);
        localStorage.setItem('newt_files_index', JSON.stringify(files));
    }
  }, [getLocalFiles]);

  const readLocalFile = useCallback((path: string): string | null => {
    return localStorage.getItem('newt_file:' + path);
  }, []);

  const deleteLocalFile = useCallback((path: string) => {
    localStorage.removeItem('newt_file:' + path);
    const files = getLocalFiles().filter(f => f !== path);
    localStorage.setItem('newt_files_index', JSON.stringify(files));
  }, [getLocalFiles]);

  const renameLocalFile = useCallback((oldPath: string, newPath: string) => {
    const content = readLocalFile(oldPath);
    if (content) {
        saveLocalFile(newPath, content);
        deleteLocalFile(oldPath);
    }
  }, [readLocalFile, saveLocalFile, deleteLocalFile]);

  const toggleTheme = async () => {
    const newTheme = theme === 'dark' ? 'light' : 'dark';
    setTheme(newTheme);
    document.documentElement.setAttribute('data-theme', newTheme);
    localStorage.setItem('newt_theme', newTheme);
  };

  const fetchConfig = useCallback(() => {
    const savedTheme = localStorage.getItem('newt_theme');
    if (savedTheme) {
      setTheme(savedTheme);
      document.documentElement.setAttribute('data-theme', savedTheme);
    }
    const savedVimMode = localStorage.getItem('newt_vim_mode');
    if (savedVimMode) {
        setVimMode(savedVimMode === 'true');
    }
  }, []);

  const toggleVimMode = () => {
      setVimMode(prev => {
          const next = !prev;
          localStorage.setItem('newt_vim_mode', String(next));
          return next;
      });
  };

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
    const local = getLocalFiles();
    setLocalFiles(local);
    
    try {
      const res = await fetch(`${API_URL}/files`);
      if (!res.ok) throw new Error(`HTTP error! status: ${res.status}`);
      const backend = await res.json();
      setBackendFiles(backend);
      setIsBackendAvailable(true);
    } catch (e) {
      console.warn("Failed to fetch backend files", e);
      setBackendFiles([]);
      setIsBackendAvailable(false);
    }
  }, [getLocalFiles]);

  useEffect(() => {
    fetchFiles();
    fetchConfig();
  }, [fetchFiles, fetchConfig]);

  // Key handler
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (inputMode === 'editing') {
        if (e.key === 'Escape') {
            setInputMode('normal');
        }
        return;
      }

      if (inputMode === 'command') {
        if (e.key === 'Enter') {
          e.preventDefault();
          await executeCommand(commandInput);
          setInputMode('normal');
          setCommandInput('');
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
        // Common navigation keys (always active)
        if (e.key === 'ArrowDown') {
            e.preventDefault(); // Prevent scrolling
            setSelectedIndices(prev => {
                const last = prev[prev.length - 1];
                const next = Math.min(last + 1, cells.length - 1);
                return [next];
            });
        } else if (e.key === 'ArrowUp') {
            e.preventDefault(); // Prevent scrolling
            setSelectedIndices(prev => {
                const last = prev[prev.length - 1];
                const next = Math.max(last - 1, 0);
                return [next];
            });
        } else if (e.key === 'ArrowLeft') {
            e.preventDefault(); // Prevent scrolling
            if (showSidebar) setFocus('sidebar');
        } else if (e.key === 'Enter') {
            if (cells.length > 0) runCell(primaryIndex);
        }

        if (vimMode) {
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
                if (showSidebar) setFocus('sidebar');
                break;
            case 'i':
                if (cells.length > 0) setInputMode('editing');
                break;
            case 'r':
                setInputMode('polling');
                setPollingInput('r');
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
        }
      } else if (focus === 'sidebar') {
        const localCount = localFiles.length;
        const computerStartIndex = localCount + 1;
        const totalItems = isBackendAvailable 
            ? localCount + 1 + backendFiles.length + 1 
            : localCount + 1;

        switch (e.key) {
          case 'j':
          case 'ArrowDown':
            setSelectedFileIndex(prev => Math.min(prev + 1, totalItems - 1));
            break;
          case 'k':
          case 'ArrowUp':
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
            if (selectedFileIndex > 0 && selectedFileIndex <= localCount) {
                setRenameInput(localFiles[selectedFileIndex - 1]);
                setInputMode('renaming');
            } else if (isBackendAvailable && selectedFileIndex > computerStartIndex) {
                setRenameInput(backendFiles[selectedFileIndex - computerStartIndex - 1]);
                setInputMode('renaming');
            }
            break;
          case 'y':
            if (selectedFileIndex > 0 && selectedFileIndex <= localCount) {
                setClipboardFile({ path: localFiles[selectedFileIndex - 1], origin: 'local' });
                setStatusMessage(`Yanked ${localFiles[selectedFileIndex - 1]}`);
            } else if (isBackendAvailable && selectedFileIndex > computerStartIndex) {
                setClipboardFile({ path: backendFiles[selectedFileIndex - computerStartIndex - 1], origin: 'backend' });
                setStatusMessage(`Yanked ${backendFiles[selectedFileIndex - computerStartIndex - 1]}`);
            }
            break;
          case 'p':
          case 'P':
            if (clipboardFile) {
                await pasteFile();
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
  }, [cells, localFiles, backendFiles, focus, inputMode, selectedIndices, selectedFileIndex, commandInput, pollingInput, clipboardCells, clipboardFile, showSidebar, renameInput, isBackendAvailable]);

  // Space chord handler
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
        if (inputMode !== 'normal') return;
        
        if (e.key === ' ') {
            e.preventDefault(); // Prevent scrolling
            spacePressedRef.current = true;
            // Reset after 1s to prevent getting stuck
            setTimeout(() => { spacePressedRef.current = false; }, 1000);
        } else if (spacePressedRef.current) {
            if (e.key === 'e') {
                e.preventDefault();
                setShowSidebar(prev => !prev);
                spacePressedRef.current = false;
            } else if (e.key === 'h' || e.key === 'ArrowLeft') {
                e.preventDefault();
                setFocus('sidebar');
                spacePressedRef.current = false;
            } else if (e.key === 'l' || e.key === 'ArrowRight') {
                e.preventDefault();
                setFocus('editor');
                spacePressedRef.current = false;
            } else {
                spacePressedRef.current = false;
            }
        }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
        window.removeEventListener('keydown', handleKeyDown);
    };
  }, [inputMode]);


  const exportNotebook = async () => {
    let md = "";
    for (const cell of cells) {
        md += "```" + cell.type + "\n";
        md += cell.content + "\n";
        md += "```\n\n";
        if (cell.output) {
            md += "Output:\n```\n" + cell.output + "\n```\n\n";
        }
    }
    const blob = new Blob([md], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = (filePath || 'notebook').replace(/\.newt$/, '') + '.md';
    a.click();
    URL.revokeObjectURL(url);
    setStatusMessage("Exported to Markdown");
  };

  const executeCommand = async (cmd: string) => {
    const parts = cmd.split(' ');
    const command = parts[0];
    const args = parts.slice(1);

    if (command === 'ra' || command === 'runall') {
        runAllCells();
    } else if (command === 'export') {
        await exportNotebook();
    } else if (command === 'w') {
        if (args.length > 0) {
            setFilePath(args[0]);
            await saveNotebook(args[0]);
        } else {
            await saveNotebook();
        }
    } else if (command === 'q') {
        // Close?
    } else if (['rust', 'cpp', 'c', 'py', 'ts', 'js'].includes(command)) {
        const langMap: Record<string, CellType> = {
            'rust': 'rust',
            'cpp': 'cpp',
            'c': 'c',
            'py': 'python',
            'ts': 'typescript',
            'js': 'javascript'
        };
        const newType = langMap[command];
        if (newType) {
            setCells(prev => {
                const newCells = [...prev];
                selectedIndices.forEach(index => {
                    if (newCells[index]) {
                        newCells[index] = { ...newCells[index], type: newType };
                    }
                });
                return newCells;
            });
            setStatusMessage(`Converted to ${newType}`);
        }
    }
  };

  const saveNotebook = async (pathOverride?: string | unknown) => {
    let path = (typeof pathOverride === 'string' ? pathOverride : undefined) || filePath;
    
    if (!path) {
        const name = window.prompt("Enter notebook name:", "notebook.newt");
        if (!name) {
            setStatusMessage("Save cancelled");
            return;
        }
        path = name;
        setFilePath(path);
    }
    
    const content = JSON.stringify(cells);
    
    if (fileOrigin === 'local') {
        saveLocalFile(path, content);
        setStatusMessage(`Saved locally to ${path}`);
        setLocalFiles(getLocalFiles());
    } else {
        try {
            await fetch(`${API_URL}/files/save`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: path, content })
            });
            setStatusMessage(`Saved to ${path}`);
            fetchFiles();
        } catch (e) {
            setStatusMessage("Backend save failed");
        }
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
    const localCount = localFiles.length;
    const computerStartIndex = localCount + 1;

    if (index === 0) {
        // New Local Notebook
        setCells([{ id: uuidv4(), content: '', output: '', type: 'python' }]);
        setFilePath(null);
        setFileOrigin('local');
        setFocus('editor');
        setSelectedIndices([0]);
    } else if (index <= localCount) {
        // Open Local File
        const file = localFiles[index - 1];
        const content = readLocalFile(file);
        if (content) {
            try {
                const loadedCells = JSON.parse(content);
                setCells(loadedCells);
                setFilePath(file);
                setFileOrigin('local');
                setFocus('editor');
                setSelectedIndices([0]);
            } catch (e) {
                setStatusMessage("Error parsing local notebook");
            }
        }
    } else if (isBackendAvailable && index === computerStartIndex) {
        // New Backend Notebook
        setCells([{ id: uuidv4(), content: '', output: '', type: 'python' }]);
        setFilePath(null);
        setFileOrigin('backend');
        setFocus('editor');
        setSelectedIndices([0]);
    } else if (isBackendAvailable && index > computerStartIndex) {
        // Open Backend File
        const file = backendFiles[index - computerStartIndex - 1];
        try {
            const res = await fetch(`${API_URL}/files/read`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: file })
            });
            if (!res.ok) throw new Error("Backend read failed");
            const content = await res.json();
            try {
                const loadedCells = JSON.parse(content);
                setCells(loadedCells);
                setFilePath(file);
                setFileOrigin('backend');
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


  const handleRename = async () => {
    const localCount = localFiles.length;
    const computerStartIndex = localCount + 1;
    const newPath = renameInput;

    if (selectedFileIndex > 0 && selectedFileIndex <= localCount) {
        const oldPath = localFiles[selectedFileIndex - 1];
        renameLocalFile(oldPath, newPath);
        fetchFiles();
        setStatusMessage(`Renamed locally to ${newPath}`);
    } else if (selectedFileIndex > computerStartIndex) {
        const oldPath = backendFiles[selectedFileIndex - computerStartIndex - 1];
        try {
            await fetch(`${API_URL}/files/rename`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ old_path: oldPath, new_path: newPath })
            });
            fetchFiles();
            setStatusMessage(`Renamed to ${newPath}`);
        } catch (e) {
            setStatusMessage("Backend rename failed");
        }
    }
  };

  const pasteFile = async () => {
    if (!clipboardFile) return;
    
    const { path: src, origin } = clipboardFile;
    let dest = src + "_copy";
    
    const localCount = localFiles.length;
    const computerStartIndex = localCount + 1;
    
    let targetOrigin: 'local' | 'backend' = 'local';
    if (selectedFileIndex >= computerStartIndex) {
        targetOrigin = 'backend';
    }
    
    let content: string | null = null;
    if (origin === 'local') {
        content = readLocalFile(src);
    } else {
        try {
            const res = await fetch(`${API_URL}/files/read`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: src })
            });
            if (res.ok) {
                content = await res.json();
            }
        } catch (e) {}
    }
    
    if (!content) {
        setStatusMessage("Failed to read source file");
        return;
    }
    
    if (targetOrigin === 'local') {
        saveLocalFile(dest, content);
        fetchFiles();
        setStatusMessage(`Pasted locally to ${dest}`);
    } else {
        try {
            await fetch(`${API_URL}/files/save`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: dest, content })
            });
            fetchFiles();
            setStatusMessage(`Pasted to ${dest}`);
        } catch (e) {
            setStatusMessage("Backend paste failed");
        }
    }
  };

  const handleFileDrop = async (source: { path: string, origin: 'local' | 'backend' }, targetOrigin: 'local' | 'backend') => {
    const { path: src, origin } = source;
    let dest = src;
    
    // Check if dest exists in target, if so append copy
    const targetFiles = targetOrigin === 'local' ? localFiles : backendFiles;
    if (targetFiles.includes(dest)) {
        dest = dest + "_copy";
    }

    let content: string | null = null;
    if (origin === 'local') {
        content = readLocalFile(src);
    } else {
        try {
            const res = await fetch(`${API_URL}/files/read`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: src })
            });
            if (res.ok) {
                content = await res.json();
            }
        } catch (e) {}
    }
    
    if (!content) {
        setStatusMessage("Failed to read source file");
        return;
    }
    
    if (targetOrigin === 'local') {
        saveLocalFile(dest, content);
        fetchFiles();
        setStatusMessage(`Copied ${src} to Browser`);
    } else {
        try {
            await fetch(`${API_URL}/files/save`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: dest, content })
            });
            fetchFiles();
            setStatusMessage(`Copied ${src} to Computer`);
        } catch (e) {
            setStatusMessage("Backend copy failed");
        }
    }
  };

  const deleteFile = async () => {
    const localCount = localFiles.length;
    const computerStartIndex = localCount + 1;

    if (selectedFileIndex > 0 && selectedFileIndex <= localCount) {
        const filename = localFiles[selectedFileIndex - 1];
        deleteLocalFile(filename);
        fetchFiles();
        setStatusMessage(`Deleted locally ${filename}`);
    } else if (selectedFileIndex > computerStartIndex) {
        const filename = backendFiles[selectedFileIndex - computerStartIndex - 1];
        try {
            await fetch(`${API_URL}/files/delete`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: filename })
            });
            fetchFiles();
            setStatusMessage(`Deleted ${filename}`);
        } catch (e) {
            setStatusMessage("Backend delete failed");
        }
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
        onToggleSidebar={() => setShowSidebar(prev => !prev)}
        onToggleTheme={toggleTheme}
        theme={theme}
        vimMode={vimMode}
        onToggleVimMode={toggleVimMode}
      />
      <div className="flex-1 flex overflow-hidden">
        <Sidebar 
            localFiles={localFiles}
            backendFiles={backendFiles}
            isBackendAvailable={isBackendAvailable}
            selectedIndex={selectedFileIndex} 
            focused={focus === 'sidebar'} 
            visible={showSidebar}
            theme={theme}
            onToggleTheme={toggleTheme}
            onFileClick={handleFileClick}
            onFileContextMenu={handleFileContextMenu}
            onFileDrop={handleFileDrop}
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
                    const localCount = localFiles.length;
                    const computerStartIndex = localCount + 1;
                    
                    if (fileContextMenu.index > 0 && fileContextMenu.index <= localCount) {
                        setRenameInput(localFiles[fileContextMenu.index - 1]);
                        setInputMode('renaming');
                    } else if (fileContextMenu.index > computerStartIndex) {
                        setRenameInput(backendFiles[fileContextMenu.index - computerStartIndex - 1]);
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
                    const localCount = localFiles.length;
                    const computerStartIndex = localCount + 1;
                    
                    if (fileContextMenu.index > 0 && fileContextMenu.index <= localCount) {
                        setClipboardFile({ path: localFiles[fileContextMenu.index - 1], origin: 'local' });
                        setStatusMessage(`Yanked ${localFiles[fileContextMenu.index - 1]}`);
                    } else if (fileContextMenu.index > computerStartIndex) {
                        setClipboardFile({ path: backendFiles[fileContextMenu.index - computerStartIndex - 1], origin: 'backend' });
                        setStatusMessage(`Yanked ${backendFiles[fileContextMenu.index - computerStartIndex - 1]}`);
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
                        pasteFile();
                    }
                    setFileContextMenu(null);
                }}
            >
                Paste
            </div>
            <div 
                className="p-1 hover:bg-bg-tertiary cursor-pointer text-sm text-red-500"
                onClick={() => {
                    deleteFile();
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

