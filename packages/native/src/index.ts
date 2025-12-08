/**
 * Neex WASM Loader
 * Universal WebAssembly loader that works in Node.js, Bun, and browsers
 */

import * as fs from 'fs';
import * as path from 'path';

// WASM Module Interface
interface NeexWasmExports {
    memory: WebAssembly.Memory;
    alloc: (len: number) => number;
    free: (ptr: number, len: number) => void;
    hash_bytes: (ptr: number, len: number) => bigint;
    hash_lo: (hash: bigint) => number;
    hash_hi: (hash: bigint) => number;
    hash_batch: (ptr: number, len: number) => bigint;
    topo_init: (nodeCount: number) => void;
    topo_add_edge: (from: number, to: number, edgeIndex: number) => boolean;
    topo_set_adj: (node: number, start: number, end: number) => void;
    topo_sort: (nodeCount: number) => number;
    topo_get_result: (index: number) => number;
    str_eq: (aPtr: number, aLen: number, bPtr: number, bLen: number) => boolean;
    str_find: (haystackPtr: number, haystackLen: number, needlePtr: number, needleLen: number) => number;
    get_version: () => number;
}

// Singleton instance
let wasmInstance: WebAssembly.Instance | null = null;
let wasmExports: NeexWasmExports | null = null;

/**
 * Find the WASM file path
 */
function findWasmPath(): string {
    const possiblePaths = [
        // Development: relative to this file
        path.join(__dirname, '../../native/zig-out/bin/neex.wasm'),
        // Production: in the package
        path.join(__dirname, '../wasm/neex.wasm'),
        // Installed as dependency
        path.join(__dirname, '../../wasm/neex.wasm'),
        // CWD based
        path.join(process.cwd(), 'node_modules/neex/wasm/neex.wasm'),
    ];

    for (const p of possiblePaths) {
        if (fs.existsSync(p)) {
            return p;
        }
    }

    throw new Error(
        `Neex WASM binary not found.\nChecked: ${possiblePaths.join('\n  - ')}`
    );
}

/**
 * Load the WASM module
 */
export async function loadWasm(): Promise<NeexWasmExports> {
    if (wasmExports) return wasmExports;

    const wasmPath = findWasmPath();
    const wasmBuffer = fs.readFileSync(wasmPath);

    const result = await WebAssembly.instantiate(wasmBuffer, {
        env: {
            // WASM may require some imports for memory management
        },
    });

    wasmInstance = result.instance;
    wasmExports = result.instance.exports as unknown as NeexWasmExports;

    // Verify WASM is loaded correctly
    const version = wasmExports.get_version();
    console.log(`[Neex] WASM engine v${(version >> 16) & 0xff}.${(version >> 8) & 0xff}.${version & 0xff} loaded`);

    return wasmExports;
}

/**
 * Get WASM memory as Uint8Array
 */
function getMemory(): Uint8Array {
    if (!wasmExports) throw new Error('WASM not loaded');
    return new Uint8Array(wasmExports.memory.buffer);
}

/**
 * Copy bytes to WASM memory
 * Returns pointer to allocated memory
 */
export function passToWasm(data: Uint8Array): { ptr: number; len: number } {
    if (!wasmExports) throw new Error('WASM not loaded');

    const len = data.length;
    const ptr = wasmExports.alloc(len);
    if (ptr === 0) throw new Error('WASM allocation failed');

    const memory = getMemory();
    memory.set(data, ptr);

    return { ptr, len };
}

/**
 * Free WASM memory
 */
export function freeWasm(ptr: number, len: number): void {
    if (!wasmExports) return;
    wasmExports.free(ptr, len);
}

/**
 * Copy bytes from WASM memory
 */
export function readFromWasm(ptr: number, len: number): Uint8Array {
    const memory = getMemory();
    return memory.slice(ptr, ptr + len);
}

// =============================================================================
// High-Level API
// =============================================================================

/**
 * Hash a Uint8Array using XXHash3
 */
export async function hashBytes(data: Uint8Array): Promise<bigint> {
    const wasm = await loadWasm();
    const { ptr, len } = passToWasm(data);

    try {
        return wasm.hash_bytes(ptr, len);
    } finally {
        freeWasm(ptr, len);
    }
}

/**
 * Hash a string
 */
export async function hashString(str: string): Promise<bigint> {
    const encoder = new TextEncoder();
    return hashBytes(encoder.encode(str));
}

/**
 * Hash a file by reading it and passing to WASM
 */
export async function hashFile(filePath: string): Promise<bigint> {
    const data = fs.readFileSync(filePath);
    return hashBytes(new Uint8Array(data));
}

/**
 * Hash multiple files in a batch (more efficient)
 */
export async function hashFilesBatch(filePaths: string[]): Promise<bigint> {
    const wasm = await loadWasm();

    // Read all files
    const chunks: Uint8Array[] = [];
    let totalLen = 0;

    for (const filePath of filePaths) {
        try {
            const data = fs.readFileSync(filePath);
            const chunk = new Uint8Array(data);
            chunks.push(chunk);
            totalLen += 4 + chunk.length; // 4 bytes for length prefix
        } catch {
            // Skip unreadable files
        }
    }

    // Build batch buffer: [len1][data1][len2][data2]...
    const batchBuffer = new Uint8Array(totalLen);
    let offset = 0;

    for (const chunk of chunks) {
        // Write length as little-endian u32
        const view = new DataView(batchBuffer.buffer);
        view.setUint32(offset, chunk.length, true);
        offset += 4;

        // Write data
        batchBuffer.set(chunk, offset);
        offset += chunk.length;
    }

    const { ptr, len } = passToWasm(batchBuffer);

    try {
        return wasm.hash_batch(ptr, len);
    } finally {
        freeWasm(ptr, len);
    }
}

/**
 * Perform topological sort on a dependency graph
 */
export async function topologicalSort(
    nodes: string[],
    edges: Array<{ from: number; to: number }>
): Promise<string[]> {
    const wasm = await loadWasm();

    wasm.topo_init(nodes.length);

    // Add edges
    let edgeIndex = 0;
    const adjLists: number[][] = nodes.map(() => []);

    for (const edge of edges) {
        wasm.topo_add_edge(edge.from, edge.to, edgeIndex);
        adjLists[edge.from].push(edgeIndex);
        edgeIndex++;
    }

    // Set adjacency boundaries
    let currentStart = 0;
    for (let i = 0; i < nodes.length; i++) {
        const adjLen = adjLists[i].length;
        wasm.topo_set_adj(i, currentStart, currentStart + adjLen);
        currentStart += adjLen;
    }

    // Sort
    const sortedCount = wasm.topo_sort(nodes.length);

    if (sortedCount < nodes.length) {
        console.warn('[Neex] Cycle detected in dependency graph');
    }

    // Get results
    const result: string[] = [];
    for (let i = 0; i < sortedCount; i++) {
        const nodeIndex = wasm.topo_get_result(i);
        result.push(nodes[nodeIndex]);
    }

    return result;
}

/**
 * Check if WASM is available
 */
export function isWasmAvailable(): boolean {
    try {
        findWasmPath();
        return true;
    } catch {
        return false;
    }
}

// Export default
export default {
    loadWasm,
    hashBytes,
    hashString,
    hashFile,
    hashFilesBatch,
    topologicalSort,
    isWasmAvailable,
};
