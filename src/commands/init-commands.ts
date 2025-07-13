// src/commands/init-commands.ts
import { spawn } from 'child_process';

export function runInit(args: string[]): void {
    // No extra logs, just run the command.
    const child = spawn('npx', ['create-neex', ...args], {
        stdio: 'inherit', // This will show the output of create-neex directly
        shell: true
    });

    child.on('close', (code) => {
        // The process exit code will be inherited from the child process.
        process.exit(code ?? 1);
    });

    child.on('error', (err) => {
        console.error('Failed to start npx create-neex:', err);
        process.exit(1);
    });
}
