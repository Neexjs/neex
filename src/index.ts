// src/index.ts - Updated version
import { Runner } from './runner';
import { RunOptions, RunResult } from './types';
import logger from './logger';

export { RunOptions, RunResult };

export interface neexOptions {
    parallel?: boolean;
    maxParallel?: number;
    printOutput?: boolean;
    color?: boolean;
    showTiming?: boolean;
    prefix?: boolean;
    stopOnError?: boolean;
    minimalOutput?: boolean;
    groupOutput?: boolean;
    isServerMode?: boolean;
    retry?: number;
    retryDelay?: number;
    registerCleanup?: (cleanupFn: () => void) => void;
}

/**
 * Run one or more commands in parallel or sequentially
 */
export async function run(commands: string | string[], options?: neexOptions): Promise<RunResult[]> {
    const cmdArray = Array.isArray(commands) ? commands : [commands];

    const runOptions: RunOptions = {
        parallel: options?.parallel ?? false,
        maxParallel: options?.maxParallel,
        printOutput: options?.printOutput ?? true,
        color: options?.color ?? true,
        showTiming: options?.showTiming ?? true,
        prefix: options?.prefix ?? true,
        stopOnError: options?.stopOnError ?? false,
        minimalOutput: options?.minimalOutput ?? false,
        groupOutput: options?.groupOutput ?? false,
        isServerMode: options?.isServerMode ?? false,
        retry: options?.retry,
        retryDelay: options?.retryDelay
    };

    const runner = new Runner(runOptions);

    if (options?.registerCleanup) {
        options.registerCleanup(() => runner.cleanup());
    }

    const results = await runner.run(cmdArray);

    if (runOptions.printOutput && cmdArray.length > 1) {
        logger.printSummary(results);
    }

    return results;
}

/**
 * Run multiple commands in parallel
 */
export async function runParallel(commands: string | string[], options?: Omit<neexOptions, 'parallel'>): Promise<RunResult[]> {
    return run(commands, { ...options, parallel: true });
}

/**
 * Run multiple commands sequentially
 */
export async function runSequential(commands: string | string[], options?: Omit<neexOptions, 'parallel'>): Promise<RunResult[]> {
    return run(commands, { ...options, parallel: false });
}

/**
 * Run multiple servers with optimized output
 */
export async function runServers(commands: string | string[], options?: Omit<neexOptions, 'parallel' | 'isServerMode'>): Promise<RunResult[]> {
    return run(commands, { 
        ...options, 
        parallel: true, 
        isServerMode: true,
        printOutput: true
    });
}

export default {
    run,
    runParallel,
    runSequential,
    runServers
};