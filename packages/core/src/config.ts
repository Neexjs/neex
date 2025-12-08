import path from 'path';
import { NeexConfig } from './types.js';
import logger from './logger.js';

export class ConfigLoader {
    static async load(rootDir: string): Promise<NeexConfig> {
        const configPath = path.join(rootDir, 'neex.json');
        const file = Bun.file(configPath);
        
        if (!await file.exists()) {
            return {};
        }

        try {
            return await file.json();
        } catch (e) {
            logger.printLine(`Failed to load neex.json: ${(e as Error).message}`, 'warn');
            return {};
        }
    }
}
