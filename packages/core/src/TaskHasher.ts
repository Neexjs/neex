import { Native } from './native.js';
import { HashingStrategy, NeexConfig, TaskConfig } from './types.js';
import { MonorepoManager } from './monorepo.js';
import crypto from 'crypto';

export class TaskHasher {
    constructor(
        private rootDir: string,
        private monorepoManager: MonorepoManager,
        private config: NeexConfig
    ) {}

    async hashTask(pkgName: string, taskName: string, taskConfig: TaskConfig): Promise<string> {
        const strategy: HashingStrategy = this.config.performance?.hashingStrategy || 'auto';
        const pkg = this.monorepoManager.getPackage(pkgName);
        
        if (!pkg) {
            throw new Error(`Package ${pkgName} not found in monorepo manager`);
        }

        const hashes: string[] = [];

        // 1. Package Source Hash (Native)
        const pkgSourceHash = Native.getPackageHash(pkg.path, strategy);
        hashes.push(`pkg:${pkgName}:${pkgSourceHash.toString()}`);

        // 2. Internal Dependencies Hash
        if (pkg.dependencies) {
            pkg.dependencies.sort().forEach((depName: string) => {
                 const depPkg = this.monorepoManager.getPackage(depName);
                 if (depPkg) {
                     const depHash = Native.getPackageHash(depPkg.path, strategy);
                     hashes.push(`dep:${depName}:${depHash.toString()}`);
                 }
            });
        }

        // 3. Task Config Hash
        if (taskConfig.cmd) hashes.push(`cmd:${taskConfig.cmd}`);
        if (taskConfig.inputs) hashes.push(`inputs:${taskConfig.inputs?.sort().join(',')}`);
        if (taskConfig.outputs) hashes.push(`outputs:${taskConfig.outputs?.sort().join(',')}`);
        hashes.push(`task:${taskName}`);

        // 4. Global Config / Lockfile
        hashes.push('v1'); 

        const combined = hashes.join('|');
        return crypto.createHash('sha256').update(combined).digest('hex');
    }
}
