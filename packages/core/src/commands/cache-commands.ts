/**
 * Cache Commands for neex CLI
 * 
 * Commands:
 *   neex cache --r2       Configure Cloudflare R2 cache
 *   neex cache --s3       Configure AWS S3 cache
 *   neex cache --status   Check cache connection status
 *   neex cache --clear    Clear cache configuration
 */

import { Command } from 'commander';
import { RemoteCacheClient, RemoteCacheConfig } from '../remote-cache';
import chalk from 'chalk';
import figures from 'figures';
import * as readline from 'readline';

export function addCacheCommands(program: Command): void {
    program
        .command('cache')
        .description('Configure and manage remote cache (R2/S3)')
        .option('--r2', 'Configure Cloudflare R2 cache')
        .option('--s3', 'Configure AWS S3 cache')
        .option('--status', 'Check cache connection status')
        .option('--clear', 'Clear cache configuration')
        .action(async (options) => {
            const cacheClient = new RemoteCacheClient(process.cwd());

            if (options.status) {
                await showStatus(cacheClient);
            } else if (options.clear) {
                await cacheClient.clearConfig();
                console.log(chalk.green(`${figures.tick} Cache configuration cleared`));
            } else if (options.r2) {
                await configureR2(cacheClient);
            } else if (options.s3) {
                await configureS3(cacheClient);
            } else {
                // Show current status by default
                await showStatus(cacheClient);
            }
        });
}

async function showStatus(client: RemoteCacheClient): Promise<void> {
    console.log(chalk.blue('\n📦 Remote Cache Status\n'));

    const status = await client.getStatus();

    if (!status.enabled) {
        console.log(chalk.yellow(`${figures.warning} Remote cache not configured`));
        console.log(chalk.gray('\nTo configure, run:'));
        console.log(chalk.cyan('  neex cache --r2   ') + chalk.gray('(Cloudflare R2)'));
        console.log(chalk.cyan('  neex cache --s3   ') + chalk.gray('(AWS S3)'));
        return;
    }

    console.log(`  Provider: ${chalk.cyan(status.provider?.toUpperCase())}`);
    console.log(`  Bucket:   ${chalk.cyan(status.bucket)}`);
    
    if (status.connected) {
        console.log(`  Status:   ${chalk.green(`${figures.tick} Connected`)}`);
    } else {
        console.log(`  Status:   ${chalk.red(`${figures.cross} Connection failed`)}`);
    }
    console.log();
}

async function configureR2(client: RemoteCacheClient): Promise<void> {
    console.log(chalk.blue('\n☁️  Configure Cloudflare R2\n'));
    console.log(chalk.gray('Get credentials from: https://dash.cloudflare.com/r2\n'));

    const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout
    });

    const question = (prompt: string): Promise<string> => {
        return new Promise((resolve) => {
            rl.question(prompt, resolve);
        });
    };

    try {
        const accountId = await question(chalk.cyan('Account ID: '));
        const accessKeyId = await question(chalk.cyan('Access Key ID: '));
        const secretAccessKey = await question(chalk.cyan('Secret Access Key: '));
        const bucket = await question(chalk.cyan('Bucket Name: '));

        rl.close();

        const config: RemoteCacheConfig = {
            provider: 'r2',
            endpoint: `https://${accountId}.r2.cloudflarestorage.com`,
            bucket,
            accessKeyId,
            secretAccessKey,
            region: 'auto'
        };

        await client.configure(config);

        console.log(chalk.green(`\n${figures.tick} R2 cache configured successfully!`));

        // Test connection
        console.log(chalk.gray('\nTesting connection...'));
        const connected = await client.checkConnection();
        
        if (connected) {
            console.log(chalk.green(`${figures.tick} Connection successful!`));
        } else {
            console.log(chalk.yellow(`${figures.warning} Could not verify connection. Check credentials.`));
        }
    } catch (error) {
        rl.close();
        console.error(chalk.red(`\n${figures.cross} Configuration failed: ${(error as Error).message}`));
        process.exit(1);
    }
}

async function configureS3(client: RemoteCacheClient): Promise<void> {
    console.log(chalk.blue('\n🪣 Configure AWS S3\n'));

    const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout
    });

    const question = (prompt: string): Promise<string> => {
        return new Promise((resolve) => {
            rl.question(prompt, resolve);
        });
    };

    try {
        const region = await question(chalk.cyan('Region (e.g., us-east-1): '));
        const bucket = await question(chalk.cyan('Bucket Name: '));
        const accessKeyId = await question(chalk.cyan('Access Key ID: '));
        const secretAccessKey = await question(chalk.cyan('Secret Access Key: '));

        rl.close();

        const config: RemoteCacheConfig = {
            provider: 's3',
            endpoint: `https://s3.${region}.amazonaws.com`,
            bucket,
            accessKeyId,
            secretAccessKey,
            region
        };

        await client.configure(config);

        console.log(chalk.green(`\n${figures.tick} S3 cache configured successfully!`));

        // Test connection
        console.log(chalk.gray('\nTesting connection...'));
        const connected = await client.checkConnection();
        
        if (connected) {
            console.log(chalk.green(`${figures.tick} Connection successful!`));
        } else {
            console.log(chalk.yellow(`${figures.warning} Could not verify connection. Check credentials.`));
        }
    } catch (error) {
        rl.close();
        console.error(chalk.red(`\n${figures.cross} Configuration failed: ${(error as Error).message}`));
        process.exit(1);
    }
}

export default addCacheCommands;
