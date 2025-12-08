import { execSync } from 'child_process';
import chalk from 'chalk';
import path from 'path';
import fs from 'fs';

export async function addPlugin(pluginName: string, options: { cwd?: string } = {}) {
  const projectPath = options.cwd || process.cwd();
  
  console.log(chalk.blue(`📦 Installing ${pluginName}...`));
  
  try {
    // Check if package.json exists
    const packageJsonPath = path.join(projectPath, 'package.json');
    if (!fs.existsSync(packageJsonPath)) {
      throw new Error('package.json not found. Make sure you are in a valid project directory.');
    }

    // Detect package manager and install
    let installCommand = `npm install ${pluginName}`;
    
    // Check if bun.lock exists (prefer bun)
    if (fs.existsSync(path.join(projectPath, 'bun.lock'))) {
      installCommand = `bun add ${pluginName}`;
    } else if (fs.existsSync(path.join(projectPath, 'pnpm-lock.yaml'))) {
      installCommand = `pnpm add ${pluginName}`;
    } else if (fs.existsSync(path.join(projectPath, 'yarn.lock'))) {
      installCommand = `yarn add ${pluginName}`;
    }

    console.log(chalk.dim(`Running: ${installCommand}`));
    
    execSync(installCommand, { 
      stdio: 'inherit',
      cwd: projectPath
    });

    // Handle specific plugins
    if (pluginName === 'neex-admin') {
      console.log(chalk.blue('🔧 Initializing admin panel...'));
      
      try {
        // Import and run the admin init function
        const adminModule = require('neex-admin');
        if (adminModule.initAdmin) {
          await adminModule.initAdmin(projectPath);
        }
        
        console.log(chalk.green('✅ Admin panel added successfully!'));
        console.log(chalk.yellow('💡 Run "npm run admin:dev" to start the admin panel'));
      } catch (error) {
        console.log(chalk.yellow('⚠️  Admin plugin installed, manual setup required'));
      }
    } else {
      // Try to find and run plugin's init function
      try {
        const pluginPath = path.join(projectPath, 'node_modules', pluginName);
        const plugin = require(pluginPath);
        
        if (plugin.init && typeof plugin.init === 'function') {
          await plugin.init(projectPath);
          console.log(chalk.green(`✅ ${pluginName} initialized successfully!`));
        } else {
          console.log(chalk.green(`✅ ${pluginName} installed successfully!`));
        }
      } catch (error) {
        console.log(chalk.green(`✅ ${pluginName} installed successfully!`));
        console.log(chalk.yellow('⚠️  No initialization function found'));
      }
    }
    
  } catch (error) {
    console.error(chalk.red(`❌ Failed to install ${pluginName}`));
    if (error instanceof Error) {
      console.error(chalk.red(error.message));
    }
    
    // Try manual installation suggestion
    console.log(chalk.yellow('💡 Try manual installation:'));
    console.log(chalk.dim(`   bun add ${pluginName}`));
    console.log(chalk.dim(`   # or npm install ${pluginName}`));
    process.exit(1);
  }
}