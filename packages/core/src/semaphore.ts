/**
 * Semaphore for concurrency control
 * Limits parallel execution to prevent resource exhaustion
 */

export class Semaphore {
    private permits: number;
    private queue: Array<() => void> = [];

    constructor(permits: number = 4) {
        this.permits = permits;
    }

    /**
     * Acquire a permit (blocks if none available)
     */
    async acquire(): Promise<void> {
        if (this.permits > 0) {
            this.permits--;
            return;
        }

        return new Promise<void>(resolve => {
            this.queue.push(resolve);
        });
    }

    /**
     * Release a permit
     */
    release(): void {
        const next = this.queue.shift();
        if (next) {
            next();
        } else {
            this.permits++;
        }
    }

    /**
     * Run a function with semaphore protection
     */
    async run<T>(fn: () => Promise<T>): Promise<T> {
        await this.acquire();
        try {
            return await fn();
        } finally {
            this.release();
        }
    }

    /**
     * Run multiple functions with concurrency control
     */
    async runAll<T>(fns: Array<() => Promise<T>>): Promise<T[]> {
        return Promise.all(fns.map(fn => this.run(fn)));
    }

    /**
     * Get available permits
     */
    get available(): number {
        return this.permits;
    }

    /**
     * Get queue length
     */
    get waiting(): number {
        return this.queue.length;
    }
}

// Default semaphore with CPU cores as limit
const cpuCount = typeof navigator !== 'undefined' 
    ? navigator.hardwareConcurrency || 4
    : (require('os').cpus?.()?.length || 4);

export const defaultSemaphore = new Semaphore(cpuCount);

export default Semaphore;
