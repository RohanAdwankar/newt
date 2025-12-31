import type { Cell } from '../components/CellList';

/**
 * Encodes a notebook (array of cells) into a URL-safe string.
 * Uses JSON serialization and base64url encoding with compression via pako.
 */
export async function encodeNotebook(cells: Cell[]): Promise<string> {
  // Create a minimal notebook state object
  const notebook = {
    cells: cells.map(cell => ({
      id: cell.id,
      content: cell.content,
      output: cell.output,
      display_data: cell.display_data,
      type: cell.type,
      polling_interval: cell.polling_interval,
      last_run: cell.last_run,
    })),
  };

  const json = JSON.stringify(notebook);

  // Compress using browser's native CompressionStream
  const encoder = new TextEncoder();
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(json));
      controller.close();
    },
  });

  const compressedStream = stream.pipeThrough(
    new CompressionStream('gzip')
  );

  // Read compressed data
  const reader = compressedStream.getReader();
  const chunks: Uint8Array[] = [];

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
  }

  // Combine chunks
  const totalLength = chunks.reduce((acc, chunk) => acc + chunk.length, 0);
  const compressed = new Uint8Array(totalLength);
  let offset = 0;
  for (const chunk of chunks) {
    compressed.set(chunk, offset);
    offset += chunk.length;
  }

  // Convert to base64url
  const base64 = btoa(String.fromCharCode(...compressed));

  // Make URL-safe: +/ → -_, strip =
  return base64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/**
 * Decodes a URL slug back into a notebook (array of cells).
 */
export async function decodeNotebook(slug: string): Promise<Cell[]> {
  try {
    // Convert from URL-safe base64 back to standard base64
    const base64 = slug.replace(/-/g, '+').replace(/_/g, '/');

    // Add padding if needed
    const padded = base64 + '==='.slice((base64.length + 3) % 4);

    // Decode base64 to binary
    const binary = atob(padded);
    const bytes = new Uint8Array([...binary].map(c => c.charCodeAt(0)));

    // Decompress using browser's native DecompressionStream
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(bytes);
        controller.close();
      },
    });

    const decompressedStream = stream.pipeThrough(
      new DecompressionStream('gzip')
    );

    // Read decompressed data
    const reader = decompressedStream.getReader();
    const chunks: Uint8Array[] = [];

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
    }

    // Combine chunks and decode to string
    const totalLength = chunks.reduce((acc, chunk) => acc + chunk.length, 0);
    const decompressed = new Uint8Array(totalLength);
    let offset = 0;
    for (const chunk of chunks) {
      decompressed.set(chunk, offset);
      offset += chunk.length;
    }

    const decoder = new TextDecoder();
    const json = decoder.decode(decompressed);

    const notebook = JSON.parse(json);
    return notebook.cells as Cell[];
  } catch (error) {
    console.error('Failed to decode notebook from URL:', error);
    throw new Error('Invalid notebook URL');
  }
}

/**
 * Extracts the notebook slug from the current URL (hash or query param).
 */
export function getSlugFromLocation(): string | null {
  // Check hash first (#n=...)
  if (typeof window !== 'undefined') {
    if (window.location.hash.startsWith('#n=')) {
      return window.location.hash.slice(3);
    }

    // Fallback to query param (?n=...)
    const params = new URLSearchParams(window.location.search);
    return params.get('n');
  }

  return null;
}
