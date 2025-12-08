import { describe, test, expect, beforeAll, afterAll } from "bun:test";
import { Runner } from "../src/runner";
import { join } from "path";
import { mkdir, writeFile, rm } from "node:fs/promises";
import { existsSync } from "node:fs";

const TEST_DIR = join(import.meta.dir, "runner-integration-test");

describe("Runner Integration (Native Bun)", () => {
    
    beforeAll(async () => {
        if (existsSync(TEST_DIR)) await rm(TEST_DIR, { recursive: true });
        await mkdir(TEST_DIR, { recursive: true });
        // Create a dummy package.json
        await writeFile(join(TEST_DIR, "package.json"), JSON.stringify({
            name: "test-pkg",
            scripts: {
                "echo-hello": "echo 'Hello Bun'",
                "exit-fail": "exit 1"
            }
        }));
    });

    afterAll(async () => {
        // Cleanup
        if (existsSync(TEST_DIR)) await rm(TEST_DIR, { recursive: true });
    });

    test("should execute simple echo command", async () => {
        const runner = new Runner({ color: false, printOutput: false }, TEST_DIR);
        const result = await runner.runCommand("echo 'Direct Command'");
        
        expect(result.success).toBe(true);
        expect(result.code).toBe(0);
        const output = result.output?.map(o => o.data).join("").trim();
        expect(output).toContain("Direct Command");
    });

    test("should execute script from package.json", async () => {
        const runner = new Runner({ color: false, printOutput: false }, TEST_DIR);
        const result = await runner.runCommand("echo-hello");
        
        expect(result.success).toBe(true);
        expect(result.code).toBe(0);
        const output = result.output?.map(o => o.data).join("").trim();
        expect(output).toContain("Hello Bun");
    });

    test("should handle failure exit code", async () => {
        const runner = new Runner({ color: false, printOutput: false }, TEST_DIR);
        const result = await runner.runCommand("exit-fail");
        
        expect(result.success).toBe(false);
        expect(result.code).toBe(1);
    });

    test("should capture stderr", async () => {
        const runner = new Runner({ color: false, printOutput: false }, TEST_DIR);
        // Using a command that writes to stderr. "echo ... >&2" isn't portable shell always but "sh -c ..." is.
        // Bun.spawn uses shell: true in Runner? Yes.
        const result = await runner.runCommand("sh -c 'echo \"Error Message\" >&2'");
        
        expect(result.success).toBe(true);
        const stderr = result.stderr?.map(o => o.data).join("").trim();
        expect(stderr).toContain("Error Message");
    });

});
