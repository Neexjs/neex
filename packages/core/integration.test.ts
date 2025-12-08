import { Runner } from './src/runner.js';
import { CacheManager } from './src/cache.js';
import path from 'path';
import { mkdir, rm, readdir } from 'node:fs/promises';

import { describe, expect, test, beforeAll, afterAll } from "bun:test";

const TEST_ROOT = path.join(process.cwd(), '.test-integration-env');

// Mock package structure
const setupTestEnv = async () => {
    if (await Bun.file(path.join(TEST_ROOT, 'package.json')).exists()) { // Check root dir?Bun.file checks file.
         // Just nuke it
         await rm(TEST_ROOT, { recursive: true, force: true });
    }
    await mkdir(TEST_ROOT, { recursive: true });
    
    // Create neex.json
    await Bun.write(path.join(TEST_ROOT, 'neex.json'), JSON.stringify({
        pipeline: {
            "build": {
                "outputs": ["dist"],
                "cache": true
            }
        },
        performance: {
            "hashingStrategy": "serial"
        }
    }, null, 2));

    // Create package.json
    await Bun.write(path.join(TEST_ROOT, 'package.json'), JSON.stringify({
        name: "test-pkg",
        version: "1.0.0",
        scripts: {
            "build": "mkdir -p dist && echo 'built' > dist/out.txt && echo 'Build Done'"
        }
    }, null, 2));
    
    // Create source file
    await Bun.write(path.join(TEST_ROOT, 'src.js'), "console.log('source');");
};

describe('Neex Full Integration', () => {
    beforeAll(async () => {
        await setupTestEnv();
    });
    afterAll(async () => {
        if (await Bun.file(path.join(TEST_ROOT, 'package.json')).exists()) await rm(TEST_ROOT, { recursive: true, force: true });
    });

    test('should execute build, cache it, and restore it', async () => {
        const originalCwd = process.cwd();
        process.chdir(TEST_ROOT);
        
        try {
            const runner = new Runner({
                printOutput: false,
                color: false
            }, TEST_ROOT);

            // 1. First Run (Miss)
            console.log("--- First Run (Expect Cache Miss) ---");
            const start1 = performance.now();
            const results1 = await runner.run(['build']);
            const end1 = performance.now();
            
            expect(results1[0].success).toBe(true);
            // expect(results1[0].output?.find(o => o.data.includes('Build Done'))).toBeDefined();
            // Output might be buffered/split differently in Bun streams?
            const out1 = results1[0].output?.map(o => o.data).join("") || "";
            expect(out1).toContain('Build Done');
            
            expect(await Bun.file('dist/out.txt').exists()).toBe(true);
            
            // Verify cache created
            const cacheManager = new CacheManager(TEST_ROOT);
            // readdir is async
            const cacheFiles = await readdir(path.join(TEST_ROOT, '.neex/cache'));
            expect(cacheFiles.length).toBeGreaterThan(0); // Should have hash dir
            
            // 2. Clean
            await rm('dist', { recursive: true, force: true });
            expect(await Bun.file('dist/out.txt').exists()).toBe(false);

            // 3. Second Run (Hit)
            console.log("--- Second Run (Expect Cache Hit) ---");
            const start2 = performance.now();
            const results2 = await runner.run(['build']);
            const end2 = performance.now();
            
            expect(results2[0].success).toBe(true);
            // Verify Replay Logs
            const combinedOutput = results2[0].output?.map(o => o.data).join('') || '';
            expect(combinedOutput).toContain('Build Done');
            
            // Verify Files Restored
            expect(await Bun.file('dist/out.txt').exists()).toBe(true);
            
            console.log(`Run 1: ${(end1 - start1).toFixed(2)}ms, Run 2: ${(end2 - start2).toFixed(2)}ms`); 
            
        } finally {
            process.chdir(originalCwd);
        }
    });
});
