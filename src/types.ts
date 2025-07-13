// src/types.ts - Updated version
export interface RunOptions {
  // Run in parallel or sequentially
  parallel: boolean;
  // Maximum number of parallel processes
  maxParallel?: number;
  // Show command output
  printOutput: boolean;
  // Color output
  color: boolean;
  // Show timing information
  showTiming: boolean;
  // Show command prefix
  prefix: boolean;
  // Stop on error
  stopOnError: boolean;
  // Use minimal output format
  minimalOutput: boolean;
  // Group output by command
  groupOutput: boolean;
  // Use server mode formatting
  isServerMode: boolean;
  // Retry options
  retry?: number;
  retryDelay?: number;
}

export interface RunResult {
  stderr: any;
  command: string;
  success: boolean;
  code: number | null;
  startTime: Date;
  endTime: Date | null;
  duration?: number;
  error?: Error;
  output?: CommandOutput[];
}

export interface CommandOutput {
  command: string;
  type: 'stdout' | 'stderr';
  data: string;
  timestamp: Date;
}

export interface ServerInfo {
  name: string;
  url?: string;
  status: 'starting' | 'running' | 'error' | 'stopped';
  pid?: number;
  port?: number;
  startTime?: Date;
}

export type LogLevel = 'info' | 'warn' | 'error' | 'debug' | 'success';