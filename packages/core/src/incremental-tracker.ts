/**
 * Incremental File Tracker
 * Tracks file modifications using mtime to avoid re-hashing unchanged files
 */

import * as fs from 'fs';
import * as path from 'path';

export interface FileState {
    path: string;
    mtime: number;
    size: number;
    hash: string;  // hex string for JSON serialization
}

export interface TrackerState {
    version: number;
    timestamp: number;
    files: Record<string, FileState>;
}

const STATE_VERSION = 1;
const STATE_FILE = '.neex/state.json';

export class IncrementalTracker {
    private state: TrackerState;
    private rootDir: string;
    private dirty = false;

    constructor(rootDir: string) {
        this.rootDir = rootDir;
        this.state = {
            version: STATE_VERSION,
            timestamp: Date.now(),
            files: {}
        };
    }

    /**
     * Load state from disk
     */
    async loadState(): Promise<void> {
        const statePath = path.join(this.rootDir, STATE_FILE);
        
        try {
            if (fs.existsSync(statePath)) {
                const data = fs.readFileSync(statePath, 'utf-8');
                const parsed = JSON.parse(data) as TrackerState;
                
                // Version check
                if (parsed.version === STATE_VERSION) {
                    this.state = parsed;
                }
            }
        } catch {
            // Ignore errors, start fresh
        }
    }

    /**
     * Save state to disk
     */
    async saveState(): Promise<void> {
        if (!this.dirty) return;

        const stateDir = path.join(this.rootDir, '.neex');
        const statePath = path.join(stateDir, 'state.json');

        try {
            if (!fs.existsSync(stateDir)) {
                fs.mkdirSync(stateDir, { recursive: true });
            }

            this.state.timestamp = Date.now();
            fs.writeFileSync(statePath, JSON.stringify(this.state, null, 2));
            this.dirty = false;
        } catch {
            // Ignore save errors
        }
    }

    /**
     * Check if a file has changed since last tracked
     */
    hasChanged(filePath: string): boolean {
        const absolutePath = path.isAbsolute(filePath) 
            ? filePath 
            : path.join(this.rootDir, filePath);

        try {
            const stat = fs.statSync(absolutePath);
            const mtime = stat.mtimeMs;
            const size = stat.size;

            const cached = this.state.files[absolutePath];
            
            if (!cached) {
                return true; // New file
            }

            // Changed if mtime or size differs
            return cached.mtime !== mtime || cached.size !== size;
        } catch {
            return true; // Error = assume changed
        }
    }

    /**
     * Get cached hash for a file (if unchanged)
     */
    getCachedHash(filePath: string): bigint | null {
        const absolutePath = path.isAbsolute(filePath) 
            ? filePath 
            : path.join(this.rootDir, filePath);

        if (this.hasChanged(absolutePath)) {
            return null;
        }

        const cached = this.state.files[absolutePath];
        if (cached?.hash) {
            return BigInt('0x' + cached.hash);
        }

        return null;
    }

    /**
     * Update file state after hashing
     */
    updateFile(filePath: string, hash: bigint): void {
        const absolutePath = path.isAbsolute(filePath) 
            ? filePath 
            : path.join(this.rootDir, filePath);

        try {
            const stat = fs.statSync(absolutePath);
            
            this.state.files[absolutePath] = {
                path: absolutePath,
                mtime: stat.mtimeMs,
                size: stat.size,
                hash: hash.toString(16)
            };
            
            this.dirty = true;
        } catch {
            // Ignore errors
        }
    }

    /**
     * Get list of changed files in a directory
     */
    async getChangedFiles(dir: string, extensions: string[] = ['.ts', '.tsx', '.js', '.jsx', '.json']): Promise<string[]> {
        const changed: string[] = [];
        const ignorePatterns = ['node_modules', '.git', 'dist', 'build', '.next', '.neex'];

        const walk = (currentDir: string): void => {
            try {
                const entries = fs.readdirSync(currentDir, { withFileTypes: true });

                for (const entry of entries) {
                    if (ignorePatterns.includes(entry.name) || entry.name.startsWith('.')) {
                        continue;
                    }

                    const fullPath = path.join(currentDir, entry.name);

                    if (entry.isDirectory()) {
                        walk(fullPath);
                    } else if (entry.isFile()) {
                        const ext = path.extname(entry.name);
                        if (extensions.includes(ext)) {
                            if (this.hasChanged(fullPath)) {
                                changed.push(fullPath);
                            }
                        }
                    }
                }
            } catch {
                // Ignore permission errors
            }
        };

        walk(dir);
        return changed;
    }

    /**
     * Get all files in directory (for first run)
     */
    async getAllFiles(dir: string, extensions: string[] = ['.ts', '.tsx', '.js', '.jsx', '.json']): Promise<string[]> {
        const files: string[] = [];
        const ignorePatterns = ['node_modules', '.git', 'dist', 'build', '.next', '.neex'];

        const walk = (currentDir: string): void => {
            try {
                const entries = fs.readdirSync(currentDir, { withFileTypes: true });

                for (const entry of entries) {
                    if (ignorePatterns.includes(entry.name) || entry.name.startsWith('.')) {
                        continue;
                    }

                    const fullPath = path.join(currentDir, entry.name);

                    if (entry.isDirectory()) {
                        walk(fullPath);
                    } else if (entry.isFile()) {
                        const ext = path.extname(entry.name);
                        if (extensions.includes(ext)) {
                            files.push(fullPath);
                        }
                    }
                }
            } catch {
                // Ignore permission errors
            }
        };

        walk(dir);
        return files;
    }

    /**
     * Clear all tracked state
     */
    clear(): void {
        this.state.files = {};
        this.dirty = true;
    }

    /**
     * Get statistics
     */
    get stats(): { trackedFiles: number; stateAge: number } {
        return {
            trackedFiles: Object.keys(this.state.files).length,
            stateAge: Date.now() - this.state.timestamp
        };
    }
}

// Singleton instance
let trackerInstance: IncrementalTracker | null = null;

export function getTracker(rootDir: string): IncrementalTracker {
    if (!trackerInstance || trackerInstance['rootDir'] !== rootDir) {
        trackerInstance = new IncrementalTracker(rootDir);
    }
    return trackerInstance;
}

export default IncrementalTracker;
