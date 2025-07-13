#!/usr/bin/env node

require('../src/cli').default();

// src/types.ts
export interface RunOptions {
    // اجرای موازی یا ترتیبی
    parallel: boolean;
    // حداکثر تعداد اجرای همزمان در حالت موازی
    maxParallel?: number;
    // نمایش خروجی هر دستور
    printOutput: boolean;
    // رنگی کردن خروجی
    color: boolean;
    // نمایش زمان اجرا
    showTiming: boolean;
    // نمایش نام اسکریپت در کنار خروجی
    prefix: boolean;
    // اگر خطا رخ دهد اجرای بقیه دستورات متوقف شود
    stopOnError: boolean;
}

export interface RunResult {
    command: string;
    success: boolean;
    code: number | null;
    startTime: Date;
    endTime: Date | null;
    duration?: number;
    error?: Error;
}

export interface CommandOutput {
    command: string;
    type: 'stdout' | 'stderr';
    data: string;
    timestamp: Date;
}
