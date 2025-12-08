import * as p from '@clack/prompts';
import * as color from 'picocolors';
import { saveGlobalConfig, getGlobalConfig, clearGlobalConfig } from './utils/config.js';

interface CacheOptions {
  r2?: boolean;
  status?: boolean;
  clear?: boolean;
}

export async function cacheCommand(options: CacheOptions): Promise<void> {
  if (options.r2) {
    await setupR2();
  } else if (options.clear) {
    await clearCache();
  } else {
    await showStatus();
  }
}

async function setupR2(): Promise<void> {
  p.intro(color.bgCyan(color.black(' Neex Remote Cache Setup ')));

  const endpoint = await p.text({
    message: 'Cloudflare R2 Endpoint URL',
    placeholder: 'https://your-account.r2.cloudflarestorage.com',
    validate: (value) => {
      if (!value) return 'Endpoint is required';
      if (!value.startsWith('http')) return 'Must be a valid URL';
    },
  });

  if (p.isCancel(endpoint)) {
    p.cancel('Setup cancelled');
    process.exit(0);
  }

  const bucket = await p.text({
    message: 'Bucket Name',
    placeholder: 'neex-cache',
    validate: (value) => {
      if (!value) return 'Bucket name is required';
    },
  });

  if (p.isCancel(bucket)) {
    p.cancel('Setup cancelled');
    process.exit(0);
  }

  const accessKeyId = await p.text({
    message: 'Access Key ID',
    validate: (value) => {
      if (!value) return 'Access Key ID is required';
    },
  });

  if (p.isCancel(accessKeyId)) {
    p.cancel('Setup cancelled');
    process.exit(0);
  }

  const secretAccessKey = await p.password({
    message: 'Secret Access Key',
    validate: (value) => {
      if (!value) return 'Secret Access Key is required';
    },
  });

  if (p.isCancel(secretAccessKey)) {
    p.cancel('Setup cancelled');
    process.exit(0);
  }

  const spinner = p.spinner();
  spinner.start('Saving configuration...');

  await saveGlobalConfig({
    r2: {
      endpoint: endpoint as string,
      bucket: bucket as string,
      accessKeyId: accessKeyId as string,
      secretAccessKey: secretAccessKey as string,
    },
  });

  spinner.stop('Configuration saved!');

  p.outro(color.green('✅ R2 Remote Cache configured successfully!'));
}

async function showStatus(): Promise<void> {
  const config = await getGlobalConfig();

  console.log(color.bold('\n📦 Neex Cache Status\n'));

  if (config.r2) {
    console.log(color.green('✓ Remote Cache: ') + color.cyan('Connected'));
    console.log(color.dim('  Endpoint: ') + config.r2.endpoint);
    console.log(color.dim('  Bucket: ') + config.r2.bucket);
  } else {
    console.log(color.yellow('○ Remote Cache: ') + color.dim('Not configured'));
    console.log(color.dim('  Run: ') + color.cyan('neex cache --r2') + color.dim(' to setup'));
  }

  console.log();
}

async function clearCache(): Promise<void> {
  const confirm = await p.confirm({
    message: 'Clear all cache configuration?',
  });

  if (p.isCancel(confirm) || !confirm) {
    p.cancel('Operation cancelled');
    return;
  }

  await clearGlobalConfig();
  p.outro(color.green('✅ Cache configuration cleared'));
}
