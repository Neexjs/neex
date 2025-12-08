import path from 'path';
import fsPromises from 'node:fs/promises';
import logger from './logger.js';
import { CommandOutput } from './types.js';

export interface CacheMeta {
    hash: string;
    exitCode: number;
    duration: number;
    timestamp: number;
    stdout: CommandOutput[];
    stderr: CommandOutput[];
}

export class CacheManager {
    private cacheDir: string;

    constructor(rootDir: string) {
        this.cacheDir = path.join(rootDir, '.neex', 'cache');
        // Lazy creation: cacheDir is created when saving
    }

    getCachePath(hash: string): string {
        return path.join(this.cacheDir, hash);
    }

    private async dirExists(dirPath: string): Promise<boolean> {
        try {
            await fsPromises.access(dirPath);
            return true;
        } catch {
            return false;
        }
    }

    async save(hash: string, outputDirs: string[], meta: CacheMeta): Promise<void> {
        const hashDir = this.getCachePath(hash);
        
        // Ensure clean slate
        if (await this.dirExists(hashDir)) {
            await fsPromises.rm(hashDir, { recursive: true, force: true });
        }
        await fsPromises.mkdir(hashDir, { recursive: true });

        // 1. Save Meta (Bun.write is faster)
        const metaPath = path.join(hashDir, 'meta.json');
        await Bun.write(metaPath, JSON.stringify(meta, null, 2));

        // 2. Save Outputs (Copy to cache)
        const filesDir = path.join(hashDir, 'files');
        
        for (const outDir of outputDirs) {
            const sourcePath = path.resolve(process.cwd(), outDir);
            const relativeOut = outDir; 
            const destPath = path.join(filesDir, relativeOut);
            
            if (await this.dirExists(sourcePath)) {
                // Use recursive copy to cache (Immutable snapshot)
                await fsPromises.cp(sourcePath, destPath, { recursive: true });
            }
        }
    }

    async restore(hash: string, outputDirs: string[]): Promise<CacheMeta | null> {
        const hashDir = this.getCachePath(hash);
        const metaPath = path.join(hashDir, 'meta.json');
        const file = Bun.file(metaPath);

        if (!await file.exists()) {
            return null;
        }

        try {
            // 1. Read Meta
            const meta: CacheMeta = await file.json();

            // 2. Replay Logs
            logger.printLine(`Replaying cache for ${hash.substring(0, 10)}...`, 'info');
            
            for (const log of meta.stdout) {
                 process.stdout.write(log.data);
            }
            for (const log of meta.stderr) {
                 process.stderr.write(log.data);
            }

            // 3. Restore Files (Hard Links)
            const filesDir = path.join(hashDir, 'files');

            for (const outDir of outputDirs) {
                const targetPath = path.resolve(process.cwd(), outDir);
                const sourcePath = path.join(filesDir, outDir);

                if (!await this.dirExists(sourcePath)) continue;

                // Nuke existing output
                if (await this.dirExists(targetPath)) {
                    await fsPromises.rm(targetPath, { recursive: true, force: true });
                }
                
                // Recursive Hard Link
                await this.hardLinkResursive(sourcePath, targetPath);
            }

            return meta;

        } catch (e) {
            logger.printLine(`Failed to restore cache: ${(e as Error).message}`, 'error');
            return null;
        }
    }

    /**
     * Recursively hard-links files from src to dest.
     * Creates directories as needed.
     */
    private async hardLinkResursive(src: string, dest: string): Promise<void> {
        // Ensure dest dir exists
        await fsPromises.mkdir(dest, { recursive: true });

        const entries = await fsPromises.readdir(src, { withFileTypes: true });

        for (const entry of entries) {
            const srcPath = path.join(src, entry.name);
            const destPath = path.join(dest, entry.name);

            if (entry.isDirectory()) {
                await this.hardLinkResursive(srcPath, destPath);
            } else if (entry.isFile()) {
                // Determine mechanism: fs.link
                // Bun doesn't have native hardlink yet, so we use node:fs/promises link
                try {
                    await fsPromises.link(srcPath, destPath);
                } catch (e: any) {
                    // Fallback to copy if cross-device link error (EXDEV)
                    if (e.code === 'EXDEV') {
                        await fsPromises.copyFile(srcPath, destPath);
                    } else {
                        throw e;
                    }
                }
            }
        }
    }
}
