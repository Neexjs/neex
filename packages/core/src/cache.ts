/**
 * Smart Cache Manager
 * 
 * Features:
 * - Content-Addressable Storage (deduplication)
 * - Gzip Compression (2.5x smaller)
 * - Hardlink Restore (zero-copy)
 * - Manifest-based tracking
 * - Remote Cache Fallback (R2/S3)
 */

import path from 'path';
import fs from 'fs';
import fsPromises from 'node:fs/promises';
import logger from './logger.js';
import { CommandOutput } from './types.js';
import { ContentStore } from './content-store';
import { RemoteCacheClient } from './remote-cache';

export interface CacheMeta {
    hash: string;
    exitCode: number;
    duration: number;
    timestamp: number;
    stdout: CommandOutput[];
    stderr: CommandOutput[];
}

export interface FileManifest {
    files: Array<{
        path: string;      // Relative path
        hash: string;      // Content hash in CAS
        size: number;      // Original size
        mode: number;      // File permissions
    }>;
    totalSize: number;
    compressedSize: number;
}

export class CacheManager {
    private cacheDir: string;
    private artifactsDir: string;
    private contentStore: ContentStore;
    private remoteCache: RemoteCacheClient;
    private rootDir: string;

    constructor(rootDir: string) {
        this.rootDir = rootDir;
        this.cacheDir = path.join(rootDir, '.neex', 'cache');
        this.artifactsDir = path.join(this.cacheDir, 'artifacts');
        this.contentStore = new ContentStore(this.cacheDir);
        this.remoteCache = new RemoteCacheClient(rootDir);
    }

    /**
     * Get artifact directory path for a task hash
     */
    getArtifactPath(hash: string): string {
        return path.join(this.artifactsDir, hash);
    }

    /**
     * Check if directory exists
     */
    private dirExists(dirPath: string): boolean {
        try {
            return fs.existsSync(dirPath) && fs.statSync(dirPath).isDirectory();
        } catch (e) {
            return false; // Stat error
        }
    }

    /**
     * Collect all files in a directory recursively
     */
    private collectFiles(dir: string, basePath: string = ''): Array<{ path: string; fullPath: string }> {
        const files: Array<{ path: string; fullPath: string }> = [];

        if (!this.dirExists(dir)) return files;

        const entries = fs.readdirSync(dir, { withFileTypes: true });

        for (const entry of entries) {
            const relativePath = path.join(basePath, entry.name);
            const fullPath = path.join(dir, entry.name);

            if (entry.isDirectory()) {
                files.push(...this.collectFiles(fullPath, relativePath));
            } else if (entry.isFile()) {
                files.push({ path: relativePath, fullPath });
            }
        }

        return files;
    }

    /**
     * Save task outputs to cache
     * Uses Content-Addressable Storage for deduplication
     */
    async save(hash: string, outputDirs: string[], meta: CacheMeta): Promise<void> {
        const artifactDir = this.getArtifactPath(hash);

        // Clean existing artifact
        if (this.dirExists(artifactDir)) {
            await fsPromises.rm(artifactDir, { recursive: true, force: true });
        }
        await fsPromises.mkdir(artifactDir, { recursive: true });

        // Build manifest
        const manifest: FileManifest = {
            files: [],
            totalSize: 0,
            compressedSize: 0
        };

        // Process each output directory
        for (const outDir of outputDirs) {
            const sourcePath = path.resolve(process.cwd(), outDir);
            
            if (!this.dirExists(sourcePath)) continue;

            const files = this.collectFiles(sourcePath);

            for (const file of files) {
                try {
                    const stat = fs.statSync(file.fullPath);
                    const content = fs.readFileSync(file.fullPath);
                    
                    // Store in CAS
                    const contentHash = await this.contentStore.put(new Uint8Array(content));

                    manifest.files.push({
                        path: path.join(outDir, file.path),
                        hash: contentHash,
                        size: stat.size,
                        mode: stat.mode
                    });

                    manifest.totalSize += stat.size;
                } catch (e) {
                    // Skip files we can't read
                    logger.printLine(`Cache: skipping ${file.path}: ${(e as Error).message}`, 'warn');
                }
            }
        }

        // Get compressed size from CAS stats
        const casStats = this.contentStore.getStats();
        manifest.compressedSize = casStats.totalSize;

        // Save meta and manifest
        const metaPath = path.join(artifactDir, 'meta.json');
        const manifestPath = path.join(artifactDir, 'manifest.json');

        await Bun.write(metaPath, JSON.stringify(meta, null, 2));
        await Bun.write(manifestPath, JSON.stringify(manifest, null, 2));

        // Log compression ratio
        if (manifest.totalSize > 0) {
            const ratio = manifest.totalSize / Math.max(manifest.compressedSize, 1);
            logger.printLine(
                `Cache saved: ${manifest.files.length} files, ` +
                `${(manifest.totalSize / 1024).toFixed(1)}KB → ${(manifest.compressedSize / 1024).toFixed(1)}KB ` +
                `(${ratio.toFixed(1)}x compression)`,
                'info'
            );
        }

        // Upload to remote cache if enabled
        if (this.remoteCache.isEnabled()) {
            try {
                const artifactData = JSON.stringify({ meta, manifest });
                await this.remoteCache.put(hash, Buffer.from(artifactData));
            } catch (e) {
                logger.printLine(`[Remote Cache] Upload failed: ${(e as Error).message}`, 'warn');
            }
        }
    }

    /**
     * Restore task outputs from cache
     * Uses hardlinks for zero-copy restore when possible
     */
    async restore(hash: string, outputDirs: string[]): Promise<CacheMeta | null> {
        const artifactDir = this.getArtifactPath(hash);
        const metaPath = path.join(artifactDir, 'meta.json');
        const manifestPath = path.join(artifactDir, 'manifest.json');

        // Check if local cache exists
        if (!fs.existsSync(metaPath) || !fs.existsSync(manifestPath)) {
            // Try remote cache fallback
            if (this.remoteCache.isEnabled()) {
                const remoteData = await this.remoteCache.get(hash);
                if (remoteData) {
                    logger.printLine(`[Remote Cache] Hit: ${hash.substring(0, 10)}...`, 'info');
                    try {
                        const { meta, manifest } = JSON.parse(remoteData.toString());
                        // Save to local cache for next time
                        await fsPromises.mkdir(artifactDir, { recursive: true });
                        await Bun.write(metaPath, JSON.stringify(meta, null, 2));
                        await Bun.write(manifestPath, JSON.stringify(manifest, null, 2));
                        // Continue with normal restore flow
                    } catch (e) {
                        logger.printLine(`[Remote Cache] Parse failed: ${(e as Error).message}`, 'warn');
                        return null;
                    }
                } else {
                    return null;
                }
            } else {
                return null;
            }
        }

        try {
            // Read meta and manifest
            const meta: CacheMeta = await Bun.file(metaPath).json();
            const manifest: FileManifest = await Bun.file(manifestPath).json();

            logger.printLine(`Restoring cache: ${hash.substring(0, 10)}... (${manifest.files.length} files)`, 'info');

            // Replay stdout/stderr
            for (const log of meta.stdout) {
                process.stdout.write(log.data);
            }
            for (const log of meta.stderr) {
                process.stderr.write(log.data);
            }

            // Restore each file from CAS
            let restored = 0;
            let failed = 0;

            for (const file of manifest.files) {
                const destPath = path.resolve(process.cwd(), file.path);

                try {
                    // Ensure directory exists
                    const destDir = path.dirname(destPath);
                    if (!fs.existsSync(destDir)) {
                        fs.mkdirSync(destDir, { recursive: true });
                    }

                    // Remove existing file
                    if (fs.existsSync(destPath)) {
                        fs.unlinkSync(destPath);
                    }

                    // Restore from CAS
                    const success = await this.contentStore.writeTo(file.hash, destPath);
                    
                    if (success) {
                        // Restore permissions
                        try {
                            fs.chmodSync(destPath, file.mode);
                        } catch (e) {
                            // Chmod permission error, ignore
                        }
                        restored++;
                    } else {
                        failed++;
                    }
                } catch (e) {
                    logger.printLine(`Failed to restore ${file.path}: ${(e as Error).message}`, 'warn');
                    failed++;
                }
            }

            logger.printLine(`Cache restored: ${restored} files (${failed} failed)`, 'info');

            return meta;

        } catch (e) {
            logger.printLine(`Failed to restore cache: ${(e as Error).message}`, 'error');
            return null;
        }
    }

    /**
     * Check if cache exists for a hash
     */
    async has(hash: string): Promise<boolean> {
        const artifactDir = this.getArtifactPath(hash);
        const metaPath = path.join(artifactDir, 'meta.json');
        return fs.existsSync(metaPath);
    }

    /**
     * Get cache statistics
     */
    getStats(): {
        artifacts: number;
        objects: number;
        totalSize: number;
        deduplicationRatio: number;
    } {
        const casStats = this.contentStore.getStats();
        
        let artifacts = 0;
        if (this.dirExists(this.artifactsDir)) {
            artifacts = fs.readdirSync(this.artifactsDir).length;
        }

        return {
            artifacts,
            objects: casStats.totalObjects,
            totalSize: casStats.totalSize,
            deduplicationRatio: artifacts > 0 ? casStats.totalObjects / artifacts : 0
        };
    }

    /**
     * Clean old cache entries
     */
    async cleanup(maxAgeDays: number = 7): Promise<{ artifacts: number; objects: number }> {
        const maxAge = maxAgeDays * 24 * 60 * 60 * 1000;
        const now = Date.now();

        let artifactsCleaned = 0;

        // Clean old artifacts
        if (this.dirExists(this.artifactsDir)) {
            const artifacts = fs.readdirSync(this.artifactsDir);
            
            for (const artifact of artifacts) {
                const artifactPath = path.join(this.artifactsDir, artifact);
                const metaPath = path.join(artifactPath, 'meta.json');
                
                try {
                    if (fs.existsSync(metaPath)) {
                        const meta: CacheMeta = JSON.parse(fs.readFileSync(metaPath, 'utf-8'));
                        
                        if (now - meta.timestamp > maxAge) {
                            await fsPromises.rm(artifactPath, { recursive: true, force: true });
                            artifactsCleaned++;
                        }
                    }
                } catch (e) {
                    // Meta read failed, clean it anyway
                    await fsPromises.rm(artifactPath, { recursive: true, force: true });
                    artifactsCleaned++;
                }
            }
        }

        // Clean orphaned objects
        const objectsCleaned = await this.contentStore.cleanup(maxAge);

        return { artifacts: artifactsCleaned, objects: objectsCleaned };
    }
}

export default CacheManager;
