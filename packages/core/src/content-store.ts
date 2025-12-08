/**
 * Content-Addressable Storage (CAS)
 * 
 * Like Git's object store:
 * - Files stored by their content hash
 * - Same content = same hash = deduplicated
 * - Compression for space efficiency
 * 
 * Structure:
 * .neex/cache/objects/
 * ├── ab/cdef1234.gz   (first 2 chars for sharding)
 * └── cd/ef567890.gz
 */

import * as fs from 'fs';
import * as path from 'path';
import { compress, decompress } from './compression';
import { hashCache } from './lru-cache';

export class ContentStore {
    private objectsDir: string;
    private initialized = false;

    constructor(cacheDir: string) {
        this.objectsDir = path.join(cacheDir, 'objects');
    }

    /**
     * Ensure objects directory exists
     */
    private async init(): Promise<void> {
        if (this.initialized) return;
        
        if (!fs.existsSync(this.objectsDir)) {
            fs.mkdirSync(this.objectsDir, { recursive: true });
        }
        this.initialized = true;
    }

    /**
     * Get object path from hash (sharded by first 2 chars)
     */
    private getObjectPath(hash: string): string {
        const prefix = hash.substring(0, 2);
        return path.join(this.objectsDir, prefix, `${hash.substring(2)}.gz`);
    }

    /**
     * Compute hash of content using XXHash64
     * Falls back to simple hash if WASM unavailable
     */
    private computeHash(content: Uint8Array): string {
        // Use FNV-1a for fast hashing (consistent with native.ts fallback)
        let hash = BigInt(0xcbf29ce484222325n);
        for (const byte of content) {
            hash ^= BigInt(byte);
            hash *= BigInt(0x100000001b3n);
            hash = hash & BigInt('0xFFFFFFFFFFFFFFFF');
        }
        return hash.toString(16).padStart(16, '0');
    }

    /**
     * Store content and return its hash
     * Deduplicates automatically - if content exists, just returns hash
     */
    async put(content: Uint8Array): Promise<string> {
        await this.init();

        const hash = this.computeHash(content);
        const objectPath = this.getObjectPath(hash);

        // Already exists = deduplicated
        if (fs.existsSync(objectPath)) {
            return hash;
        }

        // Ensure shard directory exists
        const shardDir = path.dirname(objectPath);
        if (!fs.existsSync(shardDir)) {
            fs.mkdirSync(shardDir, { recursive: true });
        }

        // Compress and write
        const compressed = compress(content);
        fs.writeFileSync(objectPath, compressed);

        return hash;
    }

    /**
     * Store file content from path
     */
    async putFile(filePath: string): Promise<string> {
        const content = fs.readFileSync(filePath);
        return this.put(new Uint8Array(content));
    }

    /**
     * Retrieve content by hash
     */
    async get(hash: string): Promise<Uint8Array | null> {
        const objectPath = this.getObjectPath(hash);

        if (!fs.existsSync(objectPath)) {
            return null;
        }

        const compressed = fs.readFileSync(objectPath);
        return decompress(new Uint8Array(compressed));
    }

    /**
     * Check if content exists
     */
    async has(hash: string): Promise<boolean> {
        const objectPath = this.getObjectPath(hash);
        return fs.existsSync(objectPath);
    }

    /**
     * Write content to file (restore)
     */
    async writeTo(hash: string, destPath: string): Promise<boolean> {
        const content = await this.get(hash);
        if (!content) return false;

        const destDir = path.dirname(destPath);
        if (!fs.existsSync(destDir)) {
            fs.mkdirSync(destDir, { recursive: true });
        }

        fs.writeFileSync(destPath, content);
        return true;
    }

    /**
     * Get storage statistics
     */
    getStats(): { totalObjects: number; totalSize: number; shards: number } {
        if (!fs.existsSync(this.objectsDir)) {
            return { totalObjects: 0, totalSize: 0, shards: 0 };
        }

        let totalObjects = 0;
        let totalSize = 0;
        let shards = 0;

        const shardDirs = fs.readdirSync(this.objectsDir, { withFileTypes: true });
        
        for (const shard of shardDirs) {
            if (!shard.isDirectory()) continue;
            shards++;

            const shardPath = path.join(this.objectsDir, shard.name);
            const objects = fs.readdirSync(shardPath);
            
            for (const obj of objects) {
                totalObjects++;
                const objPath = path.join(shardPath, obj);
                const stat = fs.statSync(objPath);
                totalSize += stat.size;
            }
        }

        return { totalObjects, totalSize, shards };
    }

    /**
     * Clean objects older than maxAge (milliseconds)
     */
    async cleanup(maxAge: number = 7 * 24 * 60 * 60 * 1000): Promise<number> {
        if (!fs.existsSync(this.objectsDir)) return 0;

        const now = Date.now();
        let cleaned = 0;

        const shardDirs = fs.readdirSync(this.objectsDir, { withFileTypes: true });

        for (const shard of shardDirs) {
            if (!shard.isDirectory()) continue;

            const shardPath = path.join(this.objectsDir, shard.name);
            const objects = fs.readdirSync(shardPath);

            for (const obj of objects) {
                const objPath = path.join(shardPath, obj);
                const stat = fs.statSync(objPath);
                
                if (now - stat.mtimeMs > maxAge) {
                    fs.unlinkSync(objPath);
                    cleaned++;
                }
            }

            // Remove empty shard directories
            const remaining = fs.readdirSync(shardPath);
            if (remaining.length === 0) {
                fs.rmdirSync(shardPath);
            }
        }

        return cleaned;
    }
}

export default ContentStore;
