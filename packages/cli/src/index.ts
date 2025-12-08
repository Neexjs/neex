#!/usr/bin/env node
import * as p from '@clack/prompts';
import * as color from 'picocolors';
import spawn from 'cross-spawn';
import { execSync, exec, type SpawnOptions } from 'child_process';
import { promisify } from 'util';
import { Command } from 'commander';
import logUpdate from 'log-update';
import fs from 'fs';
import path from 'path';

import {
  serverContent,
  envContent,
  gitignoreContent as backendGitignoreContent,
  packageJsonContent as backendPackageJsonContent,
  prismaSchemaContent,
  tsConfigContent,
} from './template/backend';
import {
  pageContent,
  layoutContent,
  globalsCssContent,
  nextConfigContent,
  nextEnvContent,
  packageJsonContent as frontendPackageJsonContent,
  eslintConfigContent,
  gitignoreContent,
  postcssConfigContent,
  tsConfigFrontendContent,
} from './template/frontend';
import {
  packageJsonContent,
  readmeContent,
  rootEnvContent,
  rootGitignore,
} from './template/root';
import {
  biomeJsonContent,
  rootPackageJsonContent as monorepoRootPackageJson,
  neexJsonContent,
  uiPackageJsonContent,
  uiButtonContent,
  tsConfigBaseContent,
  typescriptConfigPackageJson,
  typescriptConfigBase,
  biomeConfigPackageJson,
  biomeConfigBase,
} from './template/monorepo';
import { displayNeexLogo } from './utils/logo';
import { cacheCommand } from './cache-command.js';

const version = '0.1.32';

type PackageManager = 'npm' | 'pnpm' | 'yarn' | 'bun';
type ProjectType = 'express-next' | 'neex';

interface ProjectConfig {
  name: string;
  path: string;
  packageManager: PackageManager;
  type: ProjectType;
}

interface ProgressLog {
  message: string;
  success: boolean;
  timestamp?: number;
}

interface ProgressState {
  percentage: number;
  currentPackage: string;
  elapsedTime: number;
  totalCompleted: number;
  totalPackages: number;
  logs: ProgressLog[];
  isComplete: boolean;
}

function getPackageManagerCommands(packageManager: PackageManager) {
  const commands = {
    npm: {
      install: 'npm install',
      dev: 'npm run dev',
      build: 'npm run build',
      start: 'npm run start',
      prismaGenerate: 'npm run prisma:generate',
      prismaMigrate: 'npm run prisma:migrate',
    },
    pnpm: {
      install: 'pnpm install',
      dev: 'pnpm dev',
      build: 'pnpm build',
      start: 'pnpm start',
      prismaGenerate: 'pnpm prisma:generate',
      prismaMigrate: 'pnpm prisma:migrate',
    },
    yarn: {
      install: 'yarn',
      dev: 'yarn dev',
      build: 'yarn build',
      start: 'yarn start',
      prismaGenerate: 'yarn prisma:generate',
      prismaMigrate: 'yarn prisma:migrate',
    },
    bun: {
      install: 'bun install',
      dev: 'bun dev',
      build: 'bun build',
      start: 'bun start',
      prismaGenerate: 'bun prisma:generate',
      prismaMigrate: 'bun prisma:migrate',
    },
  };

  return commands[packageManager];
}

const execAsync = promisify(exec);

async function isPackageManagerInstalled(pm: PackageManager): Promise<boolean> {
  try {
    await execAsync(`${pm} --version`, { timeout: 2000 });
    return true;
  } catch {
    return false;
  }
}

function validateProjectName(name: string): string | undefined {
  if (!name || name.trim() === '') {
    return 'Project name cannot be empty.';
  }

  const normalizedName = name.toLowerCase().trim();

  if (!/^[a-z]/.test(normalizedName)) {
    return 'Project name must start with a letter (a-z).';
  }

  if (!/^[a-z0-9-_]+$/.test(normalizedName)) {
    return 'Project name can only contain lowercase letters, numbers, hyphens (-), and underscores (_).';
  }

  if (!/[a-z0-9]$/.test(normalizedName)) {
    return 'Project name must end with a letter or number.';
  }

  if (normalizedName.length > 50) {
    return 'Project name must be 50 characters or less.';
  }

  // Check for reserved names
  const reservedNames = [
    'con',
    'prn',
    'aux',
    'nul',
    'com1',
    'com2',
    'com3',
    'com4',
    'com5',
    'com6',
    'com7',
    'com8',
    'com9',
    'lpt1',
    'lpt2',
    'lpt3',
    'lpt4',
    'lpt5',
    'lpt6',
    'lpt7',
    'lpt8',
    'lpt9',
  ];
  if (reservedNames.includes(normalizedName)) {
    return `"${normalizedName}" is a reserved name and cannot be used.`;
  }

  return undefined;
}

async function executeStep(
  message: string,
  command: string,
  packageName: string,
  weight: number,
  showLogs = false,
  cwd?: string
): Promise<void> {
  progressState.currentPackage = packageName;

  return new Promise<void>((resolve, reject) => {
    const stdio = showLogs ? 'inherit' : 'ignore';
    const shellOption = process.platform === 'win32' ? 'cmd.exe' : true;

    const options: SpawnOptions = {
      shell: shellOption,
      stdio,
      cwd: cwd ? path.resolve(cwd) : process.cwd(),
      windowsHide: true,
      timeout: 60000, // 60 second timeout
    };

    const child = spawn(command, [], options);

    const timeout = setTimeout(() => {
      child.kill();
      reject(new Error(`${message} timed out after 60 seconds`));
    }, 60000);

    child.on('close', code => {
      clearTimeout(timeout);
      if (code === 0) {
        progressState.totalCompleted++;
        progressState.percentage += weight;
        progressState.logs.push({
          message: `${message} completed`,
          success: true,
          timestamp: Date.now(),
        });
        updateProgressDisplay();
        resolve();
      } else {
        progressState.logs.push({
          message: `${message} failed with exit code ${code}`,
          success: false,
          timestamp: Date.now(),
        });
        updateProgressDisplay();
        reject(new Error(`${message} failed with code ${code}`));
      }
    });

    child.on('error', error => {
      clearTimeout(timeout);
      progressState.logs.push({
        message: `${message} failed: ${error.message}`,
        success: false,
        timestamp: Date.now(),
      });
      updateProgressDisplay();
      reject(error);
    });
  });
}

async function executeStepWithRetry(
  message: string,
  command: string,
  packageName: string,
  weight: number,
  showLogs = false,
  cwd?: string,
  retries: number = 2
): Promise<void> {
  let lastError: Error | undefined;

  for (let attempt = 1; attempt <= retries; attempt++) {
    try {
      await executeStep(message, command, packageName, weight, showLogs, cwd);
      return;
    } catch (error) {
      lastError = error as Error;
      if (attempt < retries) {
        progressState.logs.push({
          message: `${message} failed, retrying (attempt ${attempt + 1}/${retries})`,
          success: false,
          timestamp: Date.now(),
        });
        updateProgressDisplay();
        await new Promise(resolve => setTimeout(resolve, 1000 * attempt)); // Exponential backoff
      }
    }
  }

  throw lastError;
}

function createRootPackageJson(
  projectPath: string,
  projectName: string,
  packageManager: PackageManager
): void {
  try {
    fs.writeFileSync(
      path.join(projectPath, 'package.json'),
      packageJsonContent(projectName, packageManager)
    );
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(`Failed to create root package.json: ${error.message}`);
    } else {
      throw new Error(
        `Failed to create root package.json: An unknown error occurred`
      );
    }
  }
}

function createRootEnvFile(projectPath: string): void {
  try {
    fs.writeFileSync(path.join(projectPath, '.env'), rootEnvContent);
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(`Failed to create root .env file: ${error.message}`);
    } else {
      throw new Error(
        `Failed to create root .env file: An unknown error occurred`
      );
    }
  }
}

function createRootGitignore(projectPath: string): void {
  try {
    fs.writeFileSync(path.join(projectPath, '.gitignore'), rootGitignore);
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(
        `Failed to create root .gitignore file: ${error.message}`
      );
    } else {
      throw new Error(
        `Failed to create root .gitignore file: An unknown error occurred`
      );
    }
  }
}

function createPrismaSchema(serverPath: string): void {
  try {
    const prismaPath = path.join(serverPath, 'prisma');
    if (!fs.existsSync(prismaPath)) {
      fs.mkdirSync(prismaPath, { recursive: true });
    }
    fs.writeFileSync(
      path.join(prismaPath, 'schema.prisma'),
      prismaSchemaContent
    );
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(`Failed to create Prisma schema: ${error.message}`);
    } else {
      throw new Error(`Failed to create Prisma schema: ${String(error)}`);
    }
  }
}

function createTsConfig(backendPath: string): void {
  try {
    fs.writeFileSync(path.join(backendPath, 'tsconfig.json'), tsConfigContent);
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(
        `Failed to create TypeScript configuration: ${error.message}`
      );
    } else {
      throw new Error(
        `Failed to create TypeScript configuration: ${String(error)}`
      );
    }
  }
}

function createNextJsFiles(frontendPath: string, extraDependencies: Record<string, string> = {}): void {
  try {
    // Create necessary directories first
    if (!fs.existsSync(frontendPath)) {
      fs.mkdirSync(frontendPath, { recursive: true });
    }

    // Create src directory and app directory
    const srcPath = path.join(frontendPath, 'src');
    const appPath = path.join(srcPath, 'app');
    fs.mkdirSync(srcPath, { recursive: true });
    fs.mkdirSync(appPath, { recursive: true });

    // Write template files
    const files = [
      { path: path.join(appPath, 'page.tsx'), content: pageContent },
      { path: path.join(appPath, 'layout.tsx'), content: layoutContent },
      { path: path.join(appPath, 'globals.css'), content: globalsCssContent },
      {
        path: path.join(frontendPath, 'next.config.ts'),
        content: nextConfigContent,
      },
      {
        path: path.join(frontendPath, 'next-env.d.ts'),
        content: nextEnvContent,
      },
      {
        path: path.join(frontendPath, 'package.json'),
        content: frontendPackageJsonContent('client', extraDependencies), // Dynamic deps
      },
      {
        path: path.join(frontendPath, 'eslint.config.mjs'),
        content: eslintConfigContent,
      },
      {
        path: path.join(frontendPath, '.gitignore'),
        content: gitignoreContent,
      },
      {
        path: path.join(frontendPath, 'postcss.config.mjs'),
        content: postcssConfigContent,
      },
      {
        path: path.join(frontendPath, 'tsconfig.json'),
        content: tsConfigFrontendContent,
      },
    ];

    files.forEach(({ path: filePath, content }) => {
      fs.writeFileSync(filePath, content);
    });
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(`Failed to create Next.js files: ${error.message}`);
    } else {
      throw new Error(`Failed to create Next.js files: ${String(error)}`);
    }
  }
}

function createExpressFiles(backendPath: string, includePrisma: boolean = true): void {
  try {
    // Ensure the directory exists
    if (!fs.existsSync(backendPath)) {
      fs.mkdirSync(backendPath, { recursive: true });
    }

    // Create src directory
    const srcPath = path.join(backendPath, 'src');
    fs.mkdirSync(srcPath, { recursive: true });

    // Write template files
    const files = [
      { path: path.join(srcPath, 'server.ts'), content: serverContent },
      { path: path.join(backendPath, '.env'), content: envContent },
      {
        path: path.join(backendPath, '.gitignore'),
        content: backendGitignoreContent,
      },
      {
        path: path.join(backendPath, 'package.json'),
        content: backendPackageJsonContent('server', includePrisma),
      },
      {
        path: path.join(backendPath, 'tsconfig.json'),
        content: tsConfigContent,
      },
    ];

    files.forEach(({ path: filePath, content }) => {
      fs.writeFileSync(filePath, content);
    });
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(`Failed to create Express files: ${error.message}`);
    } else {
      throw new Error(`Failed to create Express files: ${String(error)}`);
    }
  }
}

function createReadmeFile(
  projectPath: string,
  projectName: string,
  packageManager: PackageManager
): void {
  try {
    fs.writeFileSync(
      path.join(projectPath, 'README.md'),
      readmeContent(projectName, packageManager, 'Prisma')
    );
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(`Failed to create README file: ${error.message}`);
    } else {
      throw new Error(`Failed to create README file: ${String(error)}`);
    }
  }
}

function createPnpmWorkspace(projectPath: string): void {
  try {
    fs.writeFileSync(
      path.join(projectPath, 'pnpm-workspace.yaml'),
      `packages:
  - apps/client
  - apps/server`
    );
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(`Failed to create pnpm workspace: ${error.message}`);
    } else {
      throw new Error(`Failed to create pnpm workspace: ${String(error)}`);
    }
  }
}

const progressState: ProgressState = {
  percentage: 0,
  currentPackage: '',
  elapsedTime: 0,
  totalCompleted: 0,
  totalPackages: 0,
  logs: [],
  isComplete: false,
};

let progressInterval: NodeJS.Timeout;
let projectName = '';

function updateProgressDisplay(): void {
  const progressPercentage = Math.min(
    100,
    Math.round(progressState.percentage)
  );
  const progressBar =
    color.green('■').repeat(Math.floor(progressPercentage / 5)) +
    color.dim('□').repeat(20 - Math.floor(progressPercentage / 5));
  const formattedTime = `${progressState.elapsedTime.toFixed(1)}s`;

  let header = '';
  if (progressState.isComplete) {
    header = `${color.green('✓')} Project ${color.cyan(projectName)} created successfully`;
  } else {
    const spinnerFrames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    const spinnerFrame =
      spinnerFrames[Math.floor(Date.now() / 100) % spinnerFrames.length];
    header = `${color.cyan(spinnerFrame)} Creating project ${color.bold(color.cyan(projectName))}`;
  }

  const progressLine = `${progressBar} ${color.bold(progressPercentage + '%')} | ${color.yellow(formattedTime)} | ${color.blue(progressState.currentPackage)} | ${color.green(progressState.totalCompleted + ' completed')}`;

  // Show only the last 5 logs to avoid cluttering
  const recentLogs = progressState.logs
    .slice(-5)
    .map(
      log =>
        `${log.success ? color.green('✓') : color.red('✗')} ${color.dim(log.message)}`
    )
    .join('\n');

  logUpdate(`${header}\n${progressLine}\n${recentLogs}`);
}

function startProgressTracking(): void {
  const startTime = Date.now();
  progressInterval = setInterval(() => {
    progressState.elapsedTime = (Date.now() - startTime) / 1000;
    updateProgressDisplay();
  }, 100);
}

function stopProgressTracking(): void {
  if (progressInterval) {
    clearInterval(progressInterval);
  }
  progressState.isComplete = true;
  updateProgressDisplay();
}

async function createProjectStructure(config: ProjectConfig): Promise<void> {
  const { name, path: projectPath, packageManager } = config;

  // Define paths for the project structure
  const appsPath = path.join(projectPath, 'apps');
  const clientPath = path.join(appsPath, 'client');
  const serverPath = path.join(appsPath, 'server');

  // Create main project directory if it doesn't exist
  if (!fs.existsSync(projectPath)) {
    fs.mkdirSync(projectPath, { recursive: true });
  }

  // Create directory structure
  [appsPath, clientPath, serverPath].forEach(dir => {
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
  });

  // If pnpm is selected, create the pnpm-workspace.yaml file
  if (packageManager === 'pnpm') {
    createPnpmWorkspace(projectPath);
  }

  progressState.totalPackages = 8;
  progressState.logs = [];
  startProgressTracking();

  const steps = [
    {
      name: 'Creating root package.json',
      action: () => createRootPackageJson(projectPath, name, packageManager),
      package: 'root',
      weight: 10,
    },
    {
      name: 'Creating root .env file',
      action: () => createRootEnvFile(projectPath),
      package: 'root-env',
      weight: 10,
    },
    {
      name: 'Creating root .gitignore file',
      action: () => createRootGitignore(projectPath),
      package: 'root-gitignore',
      weight: 10,
    },
    {
      name: 'Creating Next.js frontend',
      action: () => createNextJsFiles(clientPath),
      package: 'client',
      weight: 25,
    },
    {
      name: 'Creating Express backend',
      action: () => createExpressFiles(serverPath),
      package: 'server',
      weight: 20,
    },
    {
      name: 'Creating Prisma schema',
      action: () => createPrismaSchema(serverPath),
      package: 'server-prisma',
      weight: 10,
    },
    {
      name: 'Creating TypeScript configuration',
      action: () => createTsConfig(serverPath),
      package: 'tsconfig',
      weight: 5,
    },
    {
      name: 'Creating README',
      action: () => createReadmeFile(projectPath, name, packageManager),
      package: 'docs',
      weight: 10,
    },
  ];

  for (const step of steps) {
    try {
      await executeStepWithRetry(
        step.name,
        'echo "Step completed"',
        step.package,
        step.weight,
        false
      );
      step.action();
    } catch (error) {
      if (error instanceof Error) {
        throw new Error(`Failed during ${step.name}: ${error.message}`);
      } else {
        throw new Error(`Failed during ${step.name}: ${String(error)}`);
      }
    }
  }
}

async function createMonorepoStructure(config: ProjectConfig): Promise<void> {
  const { name, path: projectPath, packageManager } = config;

  const appsPath = path.join(projectPath, 'apps');
  const packagesPath = path.join(projectPath, 'packages');
  const webPath = path.join(appsPath, 'web');
  const apiPath = path.join(appsPath, 'api');
  const uiPath = path.join(packagesPath, 'ui');
  const tsConfigPath = path.join(packagesPath, 'typescript-config');
  const biomeConfigPath = path.join(packagesPath, 'biome-config');

  // Create directories
  [projectPath, appsPath, packagesPath, webPath, apiPath, uiPath, tsConfigPath, biomeConfigPath].forEach(dir => {
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
  });

   // Create packages/ui/src directory
   const uiSrcPath = path.join(uiPath, 'src');
   if(!fs.existsSync(uiSrcPath)){
       fs.mkdirSync(uiSrcPath, { recursive: true });
   }

  progressState.totalPackages = 9;
  progressState.logs = [];
  startProgressTracking();

  const steps = [
    {
      name: 'Creating root configuration (package.json, biome, neex)',
      action: () => {
         fs.writeFileSync(path.join(projectPath, 'package.json'), monorepoRootPackageJson(name, packageManager));
         fs.writeFileSync(path.join(projectPath, 'biome.json'), biomeJsonContent);
         fs.writeFileSync(path.join(projectPath, 'neex.json'), neexJsonContent);
         fs.writeFileSync(path.join(projectPath, 'tsconfig.json'), tsConfigBaseContent);
      },
      package: 'root-config',
      weight: 15,
    },
    {
      name: 'Creating root .gitignore',
      action: () => createRootGitignore(projectPath),
      package: 'root-gitignore',
      weight: 5,
    },
    {
       name: 'Creating UI Package',
       action: () => {
           fs.writeFileSync(path.join(uiPath, 'package.json'), uiPackageJsonContent(name));
           fs.writeFileSync(path.join(uiPath, 'tsconfig.json'), tsConfigBaseContent); // Reusing base for simplicity or create specific
           fs.writeFileSync(path.join(uiSrcPath, 'button.tsx'), uiButtonContent);
       },
       package: 'packages-ui',
       weight: 10
    },
    {
       name: 'Creating Shared Configs',
       action: () => {
           // TypeScript Config
           fs.writeFileSync(path.join(tsConfigPath, 'package.json'), typescriptConfigPackageJson);
           fs.writeFileSync(path.join(tsConfigPath, 'base.json'), typescriptConfigBase);
           
           // Biome Config
           fs.writeFileSync(path.join(biomeConfigPath, 'package.json'), biomeConfigPackageJson);
           fs.writeFileSync(path.join(biomeConfigPath, 'biome.json'), biomeConfigBase);
           
           // Copy biome.json to root was done in root-config, but we can verify
       },
       package: 'packages-config',
       weight: 10
    },
    {
      name: 'Creating Next.js web app',
      action: () => createNextJsFiles(webPath, { '@repo/ui': 'workspace:*' }),
      package: 'apps-web',
      weight: 30,
    },
    {
      name: 'Creating Express api app',
      action: () => createExpressFiles(apiPath, false),
      package: 'apps-api',
      weight: 25,
    },
     // Skipping Prisma setup as requested for this mode
    {
        name: 'Finalizing setup',
        action: () => {
             // Any specific cleanup or final touches
        },
        package: 'cleanup',
        weight: 15
    }
  ];

  for (const step of steps) {
    try {
      await executeStepWithRetry(
        step.name,
        'echo "Step completed"',
        step.package,
        step.weight,
        false
      );
      step.action();
    } catch (error) {
       // Stop progress tracking on error to ensure user sees the error
       stopProgressTracking();
       if (error instanceof Error) {
        throw new Error(`Failed during ${step.name}: ${error.message}`);
      } else {
        throw new Error(`Failed during ${step.name}: ${String(error)}`);
      }
    }
  }
} // End createMonorepoStructure

async function main(): Promise<void> {
  console.clear();
  displayNeexLogo(version);
  p.intro(
    `${color.bgCyan(color.black(' Neex Project '))} ${color.dim(`create-neex v${version}`)}`
  );

  const program = new Command()
    .name('create-neex')
    .description(
      'A CLI for creating Next.js + Express TypeScript projects with Neex'
    )
    .version(`v${version}`, '-v, --version', 'Display the version number')
    .argument(
      '[projectName]',
      'Name of the project or use "." for current directory'
    )
    .option('--debug', 'Show debug logs during installation')
    .option('--npm', 'Use npm as package manager')
    .option('--pnpm', 'Use pnpm as package manager')
    .option('--yarn', 'Use yarn as package manager')
    .option('--bun', 'Use bun as package manager')
    .option('-t, --type <type>', 'Project archetype (express-next or neex)');

  program
    .command('cache')
    .description('Manage build cache settings')
    .option('--r2', 'Configure Cloudflare R2 remote cache')
    .option('--status', 'Show current cache status')
    .option('--clear', 'Clear cache configuration')
    .action(async (options) => {
      await cacheCommand(options);
      process.exit(0);
    });

  // This is key: Set the default action to run if no subcommand matched.
  // Commander will call this when there's an argument that's not a known command.
  program.action(async () => {
    // Continue with normal flow - arguments are accessed via program.args
  });

  program.parse(process.argv);

  const projectArg = program.args[0];
  const options = program.opts();
  const debugMode = options.debug || false;

  let projectPath = '';
  let inputProjectName = '';

  try {
    if (projectArg !== undefined) {
      if (projectArg === '.') {
        projectPath = process.cwd();
        inputProjectName = path.basename(projectPath);
        const errorMsg = validateProjectName(inputProjectName);
        if (errorMsg) {
          p.cancel(`${color.red('✗')} ${errorMsg}`);
          process.exit(1);
        }
      } else {
        const errorMsg = validateProjectName(projectArg);
        if (errorMsg) {
          p.cancel(`${color.red('✗')} ${errorMsg}`);
          process.exit(1);
        }
        inputProjectName = projectArg.toLowerCase().trim();
        projectPath = path.join(process.cwd(), inputProjectName);
        if (fs.existsSync(projectPath)) {
          p.cancel(
            `${color.red('✗')} A folder with the name "${inputProjectName}" already exists at ${projectPath}.`
          );
          process.exit(1);
        }
      }
    } else {
      const name = await p.text({
        message: 'Enter project name',
        placeholder: 'my-neex-app',
        validate: validateProjectName,
        defaultValue: 'my-neex-app',
      });

      if (p.isCancel(name)) {
        p.cancel('Operation cancelled.');
        process.exit(0);
      }

      inputProjectName = name.toLowerCase().trim();
      projectPath = path.join(process.cwd(), inputProjectName);
      if (fs.existsSync(projectPath)) {
        p.cancel(
          `${color.red('✗')} A folder with the name "${inputProjectName}" already exists.`
        );
        process.exit(1);
      }
    }

    // Set global project name for progress tracking
    projectName = inputProjectName;

    let projectType = options.type;

    if (!projectType) {
      projectType = await p.select({
          message: 'Select project archetype',
          options: [
              { value: 'express-next', label: 'Neex (express+nextjs)', hint: 'Optimized build system, no Prisma by default' },
              { value: 'neex', label: 'Neex Default', hint: 'Standard polyrepo-in-monorepo setup' },
          ],
          initialValue: 'express-next',
      });
    }

    if (p.isCancel(projectType)) {
        p.cancel('Operation cancelled.');
        process.exit(0);
    }

    // Determine package manager from CLI flags or prompt
    let selectedPm: PackageManager;
    
    // Check for package manager flags
    const pmFlags = {
      npm: options.npm,
      pnpm: options.pnpm,
      yarn: options.yarn,
      bun: options.bun,
    };
    
    const selectedFlags = Object.entries(pmFlags).filter(([, value]) => value);
    
    if (selectedFlags.length > 1) {
      p.cancel(`${color.red('✗')} Multiple package manager flags specified. Please use only one.`);
      process.exit(1);
    }
    
    if (selectedFlags.length === 1) {
      selectedPm = selectedFlags[0][0] as PackageManager;
      
      // Verify the selected package manager is installed
      if (!(await isPackageManagerInstalled(selectedPm))) {
        p.cancel(
          `${color.red('✗')} ${selectedPm} is not installed or not in your PATH. Please install it first.`
        );
        process.exit(1);
      }
    } else {
      // Ask for package manager interactively
      const availablePms: {
        value: PackageManager;
        label: string;
        hint?: string;
      }[] = [
        { value: 'npm', label: 'npm' },
        { value: 'pnpm', label: 'pnpm' },
        { value: 'yarn', label: 'yarn' },
        { value: 'bun', label: 'bun' },
      ];

      const pmOptions = await Promise.all(
        availablePms.map(async pm => {
          const installed = await isPackageManagerInstalled(pm.value);
          if (!installed) {
            return { ...pm, hint: color.yellow('Not detected on your system') };
          }
          return pm;
        })
      );

      const packageManager = await p.select({
        message: 'Select package manager',
        options: pmOptions,
        initialValue: 'npm',
      });

      if (p.isCancel(packageManager)) {
        p.cancel('Operation cancelled.');
        process.exit(0);
      }

      selectedPm = packageManager as PackageManager;
    }

    if (!(await isPackageManagerInstalled(selectedPm))) {
      p.note(
        `${color.yellow('Warning:')} ${selectedPm} does not seem to be installed or is not in your PATH.\n         Please ensure it is installed to be able to run project commands.`,
        color.yellow('Heads up!')
      );
    }

    const config: ProjectConfig = {
      name: inputProjectName,
      path: projectPath,
      packageManager: selectedPm,
      type: projectType as ProjectType,
    };

    const startTime = Date.now();

    if (config.type === 'express-next') {
        await createMonorepoStructure(config);
    } else {
        await createProjectStructure(config);
    }

    stopProgressTracking();
    const endTime = Date.now();
    const executionTime = ((endTime - startTime) / 1000).toFixed(2);

    const commands = getPackageManagerCommands(config.packageManager);

    const summaryMessage = `
${color.bold(color.cyan('🚀 Neex project created successfully!'))}

${color.bold('📦 Project Structure:')}
${color.cyan('➤')} ${inputProjectName}/
${color.cyan('  ├─')} .env       ${color.dim('(Root environment variables)')}
${color.cyan('  ├─')} apps/
${color.cyan('  │   ├─')} client/   ${color.dim('(Next.js frontend)')}
${color.cyan('  │   └─')} server/   ${color.dim('(Express backend)')}
${color.cyan('  │       └─')} prisma/   ${color.dim('(Database schema)')}
${color.cyan('  └─')} package.json ${color.dim('(Root configuration)')}

${color.bold('📦 Technology Stack:')}
${color.cyan('➤')} Frontend: Next.js + React  + TypeScript + Tailwind CSS
${color.cyan('➤')} Backend: Express + TypeScript + Prisma ORM
${color.cyan('➤')} Database: sqlite (configurable)

${color.bold('🚀 Next steps:')}
${color.cyan(`$ cd ${path.basename(projectPath)}`)}
${color.cyan(`$ ${commands.install}        # Install all dependencies`)}
${color.cyan(`$ ${commands.dev}        # Start development servers`)}

${color.bold(`⚠️ IMPORTANT: Run '${color.cyan(commands.install)}' to install dependencies and complete setup.`)}

${color.bold(`⏱️ Setup completed in ${color.green(executionTime + ' seconds')}`)}
`;

    p.note(
      summaryMessage,
      `${color.green('✓')} ${color.bold('Ready to start building!')}`
    );
  } catch (error) {
    stopProgressTracking();
    p.cancel(
      `${color.red('✗')} Project creation failed: ${error instanceof Error ? error.message : String(error)}`
    );
    if (debugMode) {
      console.error('\nDebug information:', error);
    }
    process.exit(1);
  }
}

// Handle graceful shutdown
process.on('SIGINT', () => {
  stopProgressTracking();
  p.cancel('Operation cancelled by user.');
  process.exit(0);
});

process.on('SIGTERM', () => {
  stopProgressTracking();
  process.exit(0);
});

// Handle unhandled promise rejections
process.on('unhandledRejection', (reason, promise) => {
  console.error('Unhandled Rejection at:', promise, 'reason:', reason);
  stopProgressTracking();
  process.exit(1);
});

main().catch(error => {
  stopProgressTracking();
  console.error('Fatal error:', error);
  process.exit(1);
});
