import os from 'os';
import fs from 'fs/promises';
import path from 'path';

export interface R2Config {
  endpoint: string;
  bucket: string;
  accessKeyId: string;
  secretAccessKey: string;
}

export interface NeexGlobalConfig {
  r2?: R2Config;
}

const CONFIG_FILE = path.join(os.homedir(), '.neexrc');

export async function saveGlobalConfig(config: NeexGlobalConfig): Promise<void> {
  const existing = await getGlobalConfig();
  const merged = { ...existing, ...config };
  
  await fs.writeFile(CONFIG_FILE, JSON.stringify(merged, null, 2), 'utf-8');
}

export async function getGlobalConfig(): Promise<NeexGlobalConfig> {
  try {
    const content = await fs.readFile(CONFIG_FILE, 'utf-8');
    return JSON.parse(content);
  } catch {
    return {};
  }
}

export async function clearGlobalConfig(): Promise<void> {
  try {
    await fs.unlink(CONFIG_FILE);
  } catch {}
}
