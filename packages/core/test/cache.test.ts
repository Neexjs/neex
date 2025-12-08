import { CacheManager, CacheMeta } from '../src/cache.js';
import path from 'path';
import fs from 'fs';
import { describe, expect, test, beforeAll, afterAll } from "bun:test";

const TEST_ROOT = path.join(process.cwd(), '.test-cache-env');

describe('CacheManager Integration', () => {
    let cacheManager: CacheManager;
    const projectDir = path.join(TEST_ROOT, 'project');
    const outDir = 'dist';
    
    beforeAll(() => {
        if (fs.existsSync(TEST_ROOT)) fs.rmSync(TEST_ROOT, { recursive: true });
        fs.mkdirSync(projectDir, { recursive: true });
        
        // Mock project files
        fs.mkdirSync(path.join(projectDir, outDir), { recursive: true });
        fs.writeFileSync(path.join(projectDir, outDir, 'main.js'), 'console.log("hello")');
        
        // Initialize CacheManager
        cacheManager = new CacheManager(projectDir);
    });
    
    afterAll(() => {
        if (fs.existsSync(TEST_ROOT)) fs.rmSync(TEST_ROOT, { recursive: true });
    });

    test('should save and restore artifacts with metadata', async () => {
        const hash = 'test-hash-123';
        const meta: CacheMeta = {
            hash,
            exitCode: 0,
            duration: 100,
            timestamp: Date.now(),
            stdout: [{ command: 'test', type: 'stdout', data: 'Build Successful', timestamp: new Date() }],
            stderr: []
        };
        
        // 1. Save
        // We must run save from CWD usually, but CacheManager takes outputDirs relative to CWD.
        // We need to change CWD or adjust CacheManager to support rootDir base for outputs?
        // CacheManager uses `path.resolve(process.cwd(), outDir)`. 
        // So we switch CWD for the test temporarily.
        const originalCwd = process.cwd();
        process.chdir(projectDir);
        
        try {
            await cacheManager.save(hash, [outDir], meta);
            
            // Verify cache existence
            const cachePath = cacheManager.getCachePath(hash);
            expect(fs.existsSync(path.join(cachePath, 'meta.json'))).toBe(true);
            expect(fs.existsSync(path.join(cachePath, 'files', outDir, 'main.js'))).toBe(true);
            
            // 2. Nuke project output
            fs.rmSync(outDir, { recursive: true });
            expect(fs.existsSync(outDir)).toBe(false);
            
            // 3. Restore
            // We can capture stdout replay? 
            // Mock process.stdout.write
            let stdoutLogs = "";
            const originalWrite = process.stdout.write;
            process.stdout.write = (chunk: any) => { stdoutLogs += chunk; return true; };
            
            const restoredMeta = await cacheManager.restore(hash, [outDir]);
            
            process.stdout.write = originalWrite;
            
            expect(restoredMeta).not.toBeNull();
            expect(restoredMeta?.hash).toBe(hash);
            expect(stdoutLogs).toContain('Build Successful');
            
            // 4. Verify Files Restored
            expect(fs.existsSync(path.join(outDir, 'main.js'))).toBe(true);
            
            // 5. Verify Hard Link (inodes match)
            const cacheFileStat = fs.statSync(path.join(cachePath, 'files', outDir, 'main.js'));
            const projectFileStat = fs.statSync(path.join(outDir, 'main.js'));
            
            console.log(`Cache Inode: ${cacheFileStat.ino}, Project Inode: ${projectFileStat.ino}`);
            expect(cacheFileStat.ino).toBe(projectFileStat.ino);
            
        } finally {
            process.chdir(originalCwd);
        }
    });
});
