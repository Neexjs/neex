export type HashingStrategy = 'auto' | 'serial' | 'parallel';

export interface PerformanceConfig {
    hashingStrategy?: HashingStrategy;
}

export interface TaskConfig {
    cmd?: string;
    dependsOn?: string[];
    inputs?: string[];
    outputs?: string[];
    cache?: boolean;
    persistent?: boolean;
}

export interface NeexConfig {
    pipeline?: Record<string, TaskConfig>;
    performance?: PerformanceConfig;
}

// Runner Types
export interface RunOptions {
    task?: string;
    args?: string[];
    concurrency?: number;
    filter?: string;
    dryRun?: boolean;
    force?: boolean;
    verbose?: boolean;
    showTiming?: boolean;
    prefix?: boolean;
    
    // Server & IO options
    isServerMode?: boolean;
    printOutput?: boolean;
    groupOutput?: boolean;
    minimalOutput?: boolean;
    color?: boolean;
    
    // Execution control
    parallel?: boolean;
    maxParallel?: number;
    stopOnError?: boolean;
    retry?: number;
    retryDelay?: number;
}

export interface RunResult {
    command: string; // Original command string
    success: boolean;
    duration: number;
    tasks?: TaskExecutionResult[]; // For grouped tasks
    
    // Single execution details
    code?: number | null;
    startTime?: Date;
    endTime?: Date | null;
    error?: Error;
    output?: CommandOutput[];
    stderr?: CommandOutput[];
}

export interface TaskExecutionResult {
    pkg: string;
    task: string;
    success: boolean;
    duration: number;
    output?: CommandOutput;
    hash?: string;
    cached?: boolean;
}

export interface CommandOutput {
    command: string;
    type: 'stdout' | 'stderr';
    data: string;
    timestamp: Date;
    // Legacy fields for compat if needed? 
    // runner.ts uses generic structure: { command, type, data, timestamp }
    stdout?: string; 
    stderr?: string;
    exitCode?: number;
}

export interface ServerInfo {
    name: string;
    port?: number;
    pid?: number;
    url?: string;
    status: 'starting' | 'running' | 'error' | 'stopped';
    startTime?: Date;
}
