import { describe, test, expect } from "bun:test";
import { getLibraryPath } from "../src/loader";
import { Native } from "../src/native";
import { Runner } from "../src/runner";
import path from "path";
import fs from "fs";

// Mock Logger to avoid cluttering output
import { loggerManager } from "../src/logger-manager";
// @ts-ignore
loggerManager.printLine = () => {}; 
// @ts-ignore
loggerManager.printStart = () => {};
// @ts-ignore
loggerManager.printEnd = () => {};
// @ts-ignore
loggerManager.printBuffer = () => {};
// @ts-ignore
loggerManager.bufferOutput = () => {};

describe("Critical Core Diagnostics", () => {
    
    test("Loader: Can resolve native binary path", () => {
        const libPath = getLibraryPath();
        console.log(`Diagnostic: Found library at ${libPath}`);
        expect(libPath).toBeString();
        // @ts-ignore
        expect(fs.existsSync(libPath)).toBe(true);
    });

    test("Native: Can load FFI and scan directory", () => {
        const rootDir = process.cwd();
        console.log(`Diagnostic: Scanning ${rootDir}`);
        const result = Native.scan(rootDir);
        
        expect(result).toBeArray();
        expect(result.length).toBeGreaterThan(0);
        console.log("Diagnostic: Scan result sample:", result.slice(0, 3));
        
        // Find packages/core in the result (result is array of strings)
        // @ts-ignore
        const corePkg = result.find((p: string) => p.includes("packages/core"));
        expect(corePkg).toBeDefined();
    });

    test("Runner: Can execute basic shell command via Bun.spawn", async () => {
        const runner = new Runner({
            parallel: false,
            printOutput: false,
            minimalOutput: true,
            isServerMode: false
        });

        console.log("Diagnostic: Running 'echo hello'");
        const result = await runner.runCommand("echo hello");
        
        expect(result.success).toBe(true);
        expect(result.code).toBe(0);
        
        const stdout = result.output?.filter(o => o.type === 'stdout').map(o => o.data).join('').trim();
        expect(stdout).toBe("hello");
    });
    
    test("Runner: Can handle complex command with arguments", async () => {
        const runner = new Runner({
            parallel: false,
            printOutput: false,
            minimalOutput: true,
            isServerMode: false
        });

        console.log("Diagnostic: Running complex command");
        process.env.VAR_TEST = "passed";
        
        try {
            const result = await runner.runCommand("echo $VAR_TEST");
            
            expect(result.success).toBe(true);
            const stdout = result.output?.filter(o => o.type === 'stdout').map(o => o.data).join('').trim();
            expect(stdout).toBe("passed");
        } finally {
            delete process.env.VAR_TEST;
        }
    });
});
