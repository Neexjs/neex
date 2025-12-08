/**
 * Neex Native Module - Optimized
 * High-performance compute with:
 * - Incremental Hashing (only changed files)
 * - LRU Cache (in-memory)
 * - Parallel Processing (with semaphore)
 * - WASM Engine (XXHash3)
 * - Bun-optimized file I/O (zero-copy when possible)
 */

import * as fs from 'fs';
import * as path from 'path';
import { HashingStrategy } from './types';
import { hashCache } from './lru-cache';
import { getTracker } from './incremental-tracker';
import { Semaphore } from './semaphore';

// WASM Module State
let wasmModule: WasmModule | null = null;
let wasmLoadError: Error | null = null;

// Check if running in Bun for optimized I/O
const isBun = typeof Bun !== 'undefined';

/**
 * Bun-optimized file reading (zero-copy when possible)
 * Falls back to Node.js fs.readFileSync if not in Bun
 */
async function readFileOptimized(filePath: string): Promise<Uint8Array> {
  if (isBun) {
    // Bun.file() uses memory-mapped I/O internally for large files
    const file = Bun.file(filePath);
    const buffer = await file.arrayBuffer();
    return new Uint8Array(buffer);
  }
  // Node.js fallback
  const data = fs.readFileSync(filePath);
  return new Uint8Array(data);
}

interface WasmModule {
    memory: WebAssembly.Memory;
    alloc: (len: number) => number;
    free: (ptr: number, len: number) => void;
    hash_bytes: (ptr: number, len: number) => bigint;
    hash_batch: (ptr: number, len: number) => bigint;
    get_version: () => number;
}

// Concurrency control
const hashSemaphore = new Semaphore(8); // 8 concurrent hash operations

/**
 * Find the WASM file path
 */
function findWasmPath(): string {
    const searchPaths = [
        path.join(__dirname, '../../native/zig-out/bin/neex.wasm'),
        path.join(__dirname, '../wasm/neex.wasm'),
        path.join(__dirname, '../../wasm/neex.wasm'),
        path.join(process.cwd(), 'node_modules/neex/wasm/neex.wasm'),
    ];

    for (const p of searchPaths) {
        if (fs.existsSync(p)) return p;
    }

    throw new Error(`Neex WASM not found. Searched:\n${searchPaths.join('\n')}`);
}

/**
 * Load WASM module lazily
 */
async function loadWasm(): Promise<WasmModule | null> {
    if (wasmModule) return wasmModule;
    if (wasmLoadError) return null;

    try {
        const wasmPath = findWasmPath();
        const wasmBuffer = fs.readFileSync(wasmPath);
        const result = await WebAssembly.instantiate(wasmBuffer, {});
        wasmModule = result.instance.exports as unknown as WasmModule;
        return wasmModule;
    } catch (error) {
        wasmLoadError = error as Error;
        return null;
    }
}

/**
 * Hash bytes using WASM
 */
async function hashBytesWasm(data: Uint8Array): Promise<bigint> {
    const wasm = await loadWasm();
    if (!wasm) return hashBytesFallback(data);

    const ptr = wasm.alloc(data.length);
    if (ptr === 0) return hashBytesFallback(data);

    try {
        const memory = new Uint8Array(wasm.memory.buffer);
        memory.set(data, ptr);
        return wasm.hash_bytes(ptr, data.length);
    } finally {
        wasm.free(ptr, data.length);
    }
}

/**
 * JavaScript fallback hash (FNV-1a)
 */
function hashBytesFallback(data: Uint8Array): bigint {
    let hash = BigInt(0xcbf29ce484222325n);
    for (const byte of data) {
        hash ^= BigInt(byte);
        hash *= BigInt(0x100000001b3n);
        hash = hash & BigInt('0xFFFFFFFFFFFFFFFF'); // Keep 64-bit
    }
    return hash;
}

/**
 * Synchronous workspace scanner
 */
function scanWorkspaceSync(rootDir: string): string[] {
    const results: string[] = [];
    const ignorePatterns = [
        'node_modules', '.git', '.next', '.turbo', '.neex',
        'dist', 'build', 'coverage', '.zig-cache', 'zig-out'
    ];

    function shouldIgnore(name: string): boolean {
        return ignorePatterns.includes(name) || name.startsWith('.');
    }

    function walk(dir: string): void {
        try {
            const entries = fs.readdirSync(dir, { withFileTypes: true });
            
            for (const entry of entries) {
                if (shouldIgnore(entry.name)) continue;

                const fullPath = path.join(dir, entry.name);

                if (entry.isDirectory()) {
                    walk(fullPath);
                } else if (entry.name === 'package.json') {
                    results.push(path.relative(rootDir, fullPath));
                }
            }
        } catch {
            // Ignore permission errors
        }
    }

    walk(rootDir);
    return results;
}

// =============================================================================
// Public API
// =============================================================================

export const Native = {
    /**
     * Scan workspace for package.json files
     */
    scan: (rootDir: string): string[] => {
        return scanWorkspaceSync(rootDir);
    },

    /**
     * Hash a single file with caching
     * Uses Bun-optimized I/O when available
     */
    hashFile: async (filePath: string): Promise<bigint> => {
        return hashSemaphore.run(async () => {
            try {
                const stat = fs.statSync(filePath);
                const cacheKey = `${filePath}:${stat.mtimeMs}:${stat.size}`;
                
                // Check LRU cache first
                const cached = hashCache.get(cacheKey);
                if (cached !== undefined) {
                    return cached;
                }
                
                // Read file using optimized I/O (Bun memory-mapped when available)
                const data = await readFileOptimized(filePath);
                const hash = await hashBytesWasm(data);
                
                // Store in cache
                hashCache.set(cacheKey, hash);
                
                return hash;
            } catch {
                return BigInt(0);
            }
        });
    },

    /**
     * Hash a single file with incremental tracking
     * Uses mtime to skip unchanged files
     */
    hashFileIncremental: async (filePath: string, rootDir: string): Promise<bigint> => {
        const tracker = getTracker(rootDir);
        
        // Check if file changed
        const cachedHash = tracker.getCachedHash(filePath);
        if (cachedHash !== null) {
            return cachedHash;
        }
        
        // Hash the file
        const hash = await Native.hashFile(filePath);
        
        // Update tracker
        tracker.updateFile(filePath, hash);
        
        return hash;
    },

    /**
     * Hash multiple files in parallel with concurrency control
     */
    hashFilesParallel: async (
        files: string[], 
        rootDir: string
    ): Promise<Map<string, bigint>> => {
        const tracker = getTracker(rootDir);
        await tracker.loadState();

        const results = new Map<string, bigint>();
        
        const hashTasks = files.map(file => async () => {
            const hash = await Native.hashFileIncremental(file, rootDir);
            results.set(file, hash);
        });

        await Promise.all(hashTasks.map(task => hashSemaphore.run(task)));
        
        // Save state for next run
        await tracker.saveState();

        return results;
    },

    /**
     * Get hash of all files in a package directory
     * Uses incremental tracking and parallel processing
     */
    getPackageHash: async (
        packageRoot: string, 
        _strategy: HashingStrategy = 'auto'
    ): Promise<bigint> => {
        const tracker = getTracker(packageRoot);
        await tracker.loadState();

        // Get all source files
        const files = await tracker.getAllFiles(packageRoot);
        
        if (files.length === 0) return BigInt(0);

        // Check what changed
        const changedFiles = files.filter(f => tracker.hasChanged(f));
        
        // If nothing changed, compute from cached hashes
        if (changedFiles.length === 0) {
            let combinedHash = BigInt(0);
            for (const file of files) {
                const cached = tracker.getCachedHash(file);
                if (cached !== null) {
                    combinedHash ^= cached;
                }
            }
            return combinedHash;
        }

        // Hash changed files in parallel using Bun-optimized I/O
        const wasm = await loadWasm();
        
        if (wasm && changedFiles.length > 0) {
            // Batch hash with WASM for maximum performance
            // Read files in parallel using Bun-optimized I/O
            const chunkPromises = changedFiles.map(async (file) => {
                try {
                    return await readFileOptimized(file);
                } catch {
                    return null;
                }
            });
            
            const chunks = (await Promise.all(chunkPromises)).filter((c): c is Uint8Array => c !== null);
            let totalLen = chunks.reduce((acc, c) => acc + 4 + c.length, 0);

            if (totalLen > 0) {
                const batchBuffer = new Uint8Array(totalLen);
                let offset = 0;

                for (const chunk of chunks) {
                    const view = new DataView(batchBuffer.buffer);
                    view.setUint32(offset, chunk.length, true);
                    offset += 4;
                    batchBuffer.set(chunk, offset);
                    offset += chunk.length;
                }

                const ptr = wasm.alloc(batchBuffer.length);
                if (ptr !== 0) {
                    try {
                        const memory = new Uint8Array(wasm.memory.buffer);
                        memory.set(batchBuffer, ptr);
                        const batchHash = wasm.hash_batch(ptr, batchBuffer.length);
                        
                        // Update tracker with batch hash
                        for (let i = 0; i < changedFiles.length; i++) {
                            tracker.updateFile(changedFiles[i], batchHash);
                        }
                        
                        await tracker.saveState();
                        return batchHash;
                    } finally {
                        wasm.free(ptr, batchBuffer.length);
                    }
                }
            }
        }

        // Fallback: parallel individual hashes
        const hashMap = await Native.hashFilesParallel(files, packageRoot);
        
        let combinedHash = BigInt(0);
        for (const hash of hashMap.values()) {
            combinedHash ^= hash;
        }

        await tracker.saveState();
        return combinedHash;
    },

    /**
     * Check if WASM engine is available
     */
    isAvailable: async (): Promise<boolean> => {
        const wasm = await loadWasm();
        return wasm !== null;
    },

    /**
     * Get engine version
     */
    getVersion: async (): Promise<string> => {
        const wasm = await loadWasm();
        if (!wasm) return 'fallback';
        const v = wasm.get_version();
        return `${(v >> 16) & 0xff}.${(v >> 8) & 0xff}.${v & 0xff}`;
    },

    /**
     * Get performance statistics
     */
    getStats: (): { cache: { hits: number; misses: number; hitRate: number; size: number } } => {
        return {
            cache: hashCache.stats
        };
    },

    /**
     * Clear all caches (for testing)
     */
    clearCache: (): void => {
        hashCache.clear();
    }
};

export default Native;
