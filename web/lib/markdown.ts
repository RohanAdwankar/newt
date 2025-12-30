import { Cell, CellType } from '../components/CellList';
import { v4 as uuidv4 } from 'uuid';

export function parseMarkdown(content: string): Cell[] {
  const cells: Cell[] = [];
  let currentText = '';
  const lines = content.split('\n');
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.trimStart().startsWith('```')) {
      // Flush current text as Markdown cell
      if (currentText.trim().length > 0) {
        const id = uuidv4();
        // Remove leading newline if present
        let cellContent = currentText.startsWith('\n') ? currentText.slice(1) : currentText;
        cellContent = cellContent.trimEnd();

        cells.push({
          id,
          content: cellContent,
          output: '',
          type: 'markdown'
        });
        currentText = '';
      }

      // Parse Code Block
      const langTag = line.trimStart().replace('```', '').trim();

      const cellType: CellType = (() => {
        switch (langTag) {
          case 'rust': return 'rust';
          case 'python':
          case 'py': return 'python';
          case 'javascript':
          case 'js': return 'javascript';
          case 'typescript':
          case 'ts': return 'typescript';
          case 'c': return 'c';
          case 'cpp':
          case 'c++': return 'cpp';
          case 'go': return 'go';
          case 'bash':
          case 'sh':
          case 'shell': return 'shell';
          case 'markdown':
          case 'md': return 'markdown';
          default: return 'shell'; // if codeblock with no specific tag then it's shell
        }
      })();

      // Collect code content
      let codeContent = '';
      i++; // Move past opening fence
      while (i < lines.length) {
        if (lines[i].trimStart().startsWith('```')) {
          i++; // Consume closing fence
          break;
        }
        codeContent += lines[i] + '\n';
        i++;
      }

      // Parse Output (lines starting with ">> ")
      let outputContent = '';
      while (i < lines.length && lines[i].startsWith('>> ')) {
        outputContent += lines[i].slice(3) + '\n';
        i++;
      }

      const id = uuidv4();

      if (cellType === 'markdown') {
        cells.push({
          id,
          content: codeContent.trimEnd(),
          output: '',
          type: cellType
        });
      } else {
        cells.push({
          id,
          content: codeContent.trimEnd(),
          output: outputContent.trimEnd(),
          type: cellType
        });
      }
    } else {
      currentText += line + '\n';
      i++;
    }
  }

  // Flush remaining text
  if (currentText.trim().length > 0) {
    const id = uuidv4();
    // Remove leading newline if present
    let cellContent = currentText.startsWith('\n') ? currentText.slice(1) : currentText;
    cellContent = cellContent.trimEnd();

    cells.push({
      id,
      content: cellContent,
      output: '',
      type: 'markdown'
    });
  }

  // Ensure at least one cell
  if (cells.length === 0) {
    cells.push({
      id: uuidv4(),
      content: '',
      output: '',
      type: 'shell'
    });
  }

  return cells;
}

export function toMarkdown(cells: Cell[]): string {
  let output = '';

  for (let i = 0; i < cells.length; i++) {
    const cell = cells[i];

    if (i > 0) {
      output += '\n';
    }

    if (cell.type === 'markdown') {
      output += cell.content;
      output += '\n';
    } else {
      const lang: string = (() => {
        const t = cell.type;
        if (t === 'rust') return 'rust';
        if (t === 'python') return 'python';
        if (t === 'javascript') return 'javascript';
        if (t === 'typescript') return 'typescript';
        if (t === 'c') return 'c';
        if (t === 'cpp') return 'cpp';
        if (t === 'go') return 'go';
        if (t === 'shell') return 'bash';
        return 'bash';
      })();

      output += `\`\`\`${lang}\n`;
      output += cell.content;
      if (!cell.content.endsWith('\n')) {
        output += '\n';
      }
      output += '```\n';

      if (cell.output && cell.output.length > 0) {
        for (const line of cell.output.split('\n')) {
          output += `>> ${line}\n`;
        }
      }
    }
  }

  return output;
}
